//! Cells (MATLAB's `heatmap` and `histogram2` drawn as tiles, Matplotlib's `pcolormesh` on a rectilinear grid, and
//! the occupancy matrix of #74): where each cell is drawn, how the gap insets it, what colour its value takes through
//! its scale, what is drawn for a value the scale cannot place, and the automatic limits cells set.
//!
//! A cell is read by where it lies and what colour it is, so every geometric test maps the drawn rectangle back
//! through the axis maps of its axes, as `bars` does, and compares it in data units with the edges the figure holds.
//! Where a test must know which cell a rectangle is, each cell is given its own entry of an indexed palette of
//! distinct colours and found by its colour, so that the test checks that cell `(j, i)` lies between column edges
//! `i` and `i + 1` and row edges `j` and `j + 1` rather than only that some cell lies there. Expected colours are
//! worked out here from the colours of the scale and the arithmetic of the schema, not taken from the compiler.

use ironlab_ir::{
    CategoricalColorScale, CellEdges, CellGap, Color, ColorCategory, ColorScaleKind,
    ColorScaleLevels, ColorSpec, Colormap, ContinuousColorScale, DiscreteColorScale,
    IndexedColorScale, Limits, NodeId, OutOfRange, Scale, Text, View3d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{Item, ItemKind, Rect, Rgba, Stroke};
use ironlab_scene::maths::colormap::VIRIDIS;

use crate::common::{Fx, compile_figure, nearest_lut_index, out_of_range, rgb8, rgba};
use crate::probe::{assert_close, axis_maps, bbox_of, from_source, leaves};

/// Agreement expected of a rectangle mapped from data to the page and back.
const TOL: f64 = 1e-9;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const YELLOW: Color = Color::rgb(1.0, 1.0, 0.0);
const CYAN: Color = Color::rgb(0.0, 1.0, 1.0);
const MAGENTA: Color = Color::rgb(1.0, 0.0, 1.0);
const GREY: Color = Color::rgb(0.5, 0.5, 0.5);
const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);

/// Six distinct colours, one per cell of a 2 × 3 grid, so that a drawn cell can be told by its colour.
const DISTINCT: [Color; 6] = [RED, GREEN, BLUE, YELLOW, CYAN, MAGENTA];

// ---------------------------------------------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------------------------------------------

/// A cell as it was drawn: its rectangle on the page, the same rectangle as data along x and along y (each as
/// `[low, high]`), and how it was painted.
///
/// Cells may be drawn one path per cell, or several cells of one colour as one path of several rectangles, and a
/// cell may be drawn as one path that is both filled and stroked or as a filled path and a stroked path over the same
/// rectangle. Every rectangle is taken here as one cell and paths over the same rectangle are merged, so that the
/// tests do not depend on those choices. A paint whose alpha is 0 paints nothing and is ignored.
#[derive(Clone, Debug)]
struct Drawn {
    page: Rect,
    x: [f64; 2],
    y: [f64; 2],
    fill: Option<Rgba>,
    stroke: Option<Stroke>,
}

fn sorted2(a: f64, b: f64) -> [f64; 2] {
    if a <= b { [a, b] } else { [b, a] }
}

fn same_rect(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() < 1e-6
        && (a.y - b.y).abs() < 1e-6
        && (a.width - b.width).abs() < 1e-6
        && (a.height - b.height).abs() < 1e-6
}

/// Returns the cells that `cells` drew in the 2D axes `ax`, in paint order. Every subpath it drew must be a closed
/// axis-aligned rectangle, because that is what a cell is.
fn drawn(scene: &Scene, ax: NodeId, cells: NodeId) -> Vec<Drawn> {
    let (xmap, ymap) = axis_maps(scene, ax);
    let mut out: Vec<Drawn> = Vec::new();
    for leaf in from_source(&leaves(scene), cells) {
        let Some(path) = leaf.path() else {
            panic!("cells are drawn as paths, found {:?}", leaf.kind);
        };
        let fill = path.fill.map(|f| f.color).filter(|c| c.a > 0.0);
        let stroke = path.stroke.clone().filter(|s| s.color.a > 0.0);
        if fill.is_none() && stroke.is_none() {
            continue;
        }
        for corners in leaf.subpaths() {
            assert_eq!(corners.len(), 4, "a cell is a rectangle: {corners:?}");
            assert!(
                (0..4).all(|k| {
                    let (a, b) = (corners[k], corners[(k + 1) % 4]);
                    (a.x - b.x).abs() < 1e-9 || (a.y - b.y).abs() < 1e-9
                }),
                "a cell is an axis-aligned rectangle: {corners:?}"
            );
            let page = bbox_of(corners).expect("a rectangle has a box");
            if let Some(same) = out.iter_mut().find(|d| same_rect(d.page, page)) {
                same.fill = same.fill.or(fill);
                same.stroke = same.stroke.clone().or(stroke.clone());
            } else {
                out.push(Drawn {
                    page,
                    x: sorted2(xmap.to_data(page.x), xmap.to_data(page.right())),
                    y: sorted2(ymap.to_data(page.y), ymap.to_data(page.bottom())),
                    fill,
                    stroke: stroke.clone(),
                });
            }
        }
    }
    out
}

/// Returns the one cell whose extent in data contains `(x, y)`, or a panic naming the place.
#[track_caller]
fn at(cells: &[Drawn], x: f64, y: f64) -> &Drawn {
    let found: Vec<&Drawn> = cells.iter().filter(|d| contains(d, x, y)).collect();
    assert_eq!(found.len(), 1, "one cell covers ({x}, {y}): {cells:#?}");
    found[0]
}

/// Returns whether any drawn cell covers `(x, y)`.
fn covers(cells: &[Drawn], x: f64, y: f64) -> bool {
    cells.iter().any(|d| contains(d, x, y))
}

fn contains(d: &Drawn, x: f64, y: f64) -> bool {
    d.x[0] < x && x < d.x[1] && d.y[0] < y && y < d.y[1]
}

/// Returns the one cell filled in `colour`, or a panic naming the colour.
#[track_caller]
fn filled_in(cells: &[Drawn], colour: Color) -> &Drawn {
    let found: Vec<&Drawn> = cells
        .iter()
        .filter(|d| d.fill.is_some_and(|f| rgb8(f) == rgb8(rgba(colour))))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "one cell is filled in {colour:?}: {cells:#?}"
    );
    found[0]
}

#[track_caller]
fn assert_span(actual: [f64; 2], expected: [f64; 2], what: &str) {
    assert!(
        (actual[0] - expected[0]).abs() <= TOL && (actual[1] - expected[1]).abs() <= TOL,
        "{what}: drawn across {actual:?}, expected {expected:?}"
    );
}

/// Asserts that a colour is the 8-bit colour `expected`, within `slack` levels per channel.
#[track_caller]
fn assert_colour(actual: Option<Rgba>, expected: [u8; 3], slack: u8, what: &str) {
    let actual = actual.unwrap_or_else(|| panic!("{what}: nothing is painted"));
    let got = rgb8(actual);
    assert!(
        got.iter()
            .zip(expected)
            .all(|(a, e)| a.abs_diff(e) <= slack),
        "{what}: painted {got:?}, expected {expected:?}"
    );
}

/// The 8-bit colour of an IR colour.
fn rgb8_of(color: Color) -> [u8; 3] {
    rgb8(rgba(color))
}

/// The messages of the warnings that name `id`.
fn warnings_naming(scene: &Scene, id: NodeId) -> Vec<String> {
    scene
        .warnings
        .iter()
        .filter(|w| w.node == Some(id))
        .map(|w| w.message.clone())
        .collect()
}

/// The dense groups of the scene, in paint order, as `(source, cells)`.
fn dense_groups(scene: &Scene) -> Vec<(Option<NodeId>, u64)> {
    fn walk(items: &[Item], out: &mut Vec<(Option<NodeId>, u64)>) {
        for item in items {
            match &item.kind {
                ItemKind::Dense { cells, .. } => out.push((item.source, *cells)),
                ItemKind::Group { items, .. } | ItemKind::Depth { items } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&scene.display_list.items, &mut out);
    out
}

// ---------------------------------------------------------------------------------------------------------------
// Scales
// ---------------------------------------------------------------------------------------------------------------

/// An indexed scale of the given colours, strict outside them.
fn palette(colors: &[Color]) -> ColorScaleKind {
    ColorScaleKind::Indexed(IndexedColorScale {
        colors: colors.to_vec(),
        ..IndexedColorScale::default()
    })
}

/// A categorical scale of the given colours, the category of entry `k` labelled "status k", strict outside them.
fn statuses(colors: &[Color]) -> ColorScaleKind {
    ColorScaleKind::Categorical(CategoricalColorScale {
        categories: colors
            .iter()
            .enumerate()
            .map(|(k, &color)| ColorCategory {
                color,
                label: Text::plain(format!("status {k}")),
            })
            .collect(),
        outside: out_of_range(OutOfRange::Strict),
    })
}

/// A continuous scale interpolating linearly in sRGB from red at 0 to blue at 1, whose ends are unmistakable.
fn red_to_blue() -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, BLUE],
        },
        limits: Limits::Manual { min: 0.0, max: 1.0 },
        ..ContinuousColorScale::default()
    })
}

/// The colour of the red-to-blue colormap at the normalised position `t`, computed independently: a linear
/// interpolation in sRGB.
fn red_to_blue_at(t: f64) -> [u8; 3] {
    [
        ((1.0 - t) * 255.0).round() as u8,
        0,
        (t * 255.0).round() as u8,
    ]
}

/// An axes holding nothing yet.
fn axes() -> (Fx, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    (fx, ax)
}

/// A 2 × 3 grid of cells on unit edges in a fresh axes, cell `(j, i)` holding the index `3j + i` of its own colour
/// of [`DISTINCT`], edited by `edit`. Returns the scene, the axes and the cells.
fn distinct_grid(edit: impl FnOnce(&mut ironlab_ir::Cells, &mut Fx)) -> (Scene, NodeId, NodeId) {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(palette(&DISTINCT));
    let cells = fx.cells(
        ax,
        vec![2, 3],
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        Some(scale),
        edit,
    );
    (compile_figure(&fx.build()), ax, cells)
}

// ---------------------------------------------------------------------------------------------------------------
// Where cells are drawn
// ---------------------------------------------------------------------------------------------------------------

// Why: cells were skipped with a warning ("cells artists are not drawn yet") until the compiler could draw them; a
// heatmap that compiles to nothing, or that carries a warning a front end shows as a problem, tells the reader the
// figure is broken. One rectangle per cell, and no warning naming the cells, is the minimum a heatmap owes its reader.
#[test]
fn cells_are_drawn_as_one_rectangle_per_cell_without_a_warning() {
    let (scene, ax, cells) = distinct_grid(|_, _| {});
    assert_eq!(drawn(&scene, ax, cells).len(), 6, "one rectangle per cell");
    assert!(
        warnings_naming(&scene, cells).is_empty(),
        "valid cells compile without a warning: {:?}",
        scene.warnings
    );
}

// Why: a matrix is read cell by cell, so every value must be drawn exactly at its own place: with unit edges cell
// `(j, i)` spans `i − 0.5` to `i + 0.5` and `j − 0.5` to `j + 0.5`, so that its centre lies at its indices, as the
// pixels of an image without a range do, and row 0 lies between the first two row edges. Seamless cells (a gap of
// 0, the default) touch their neighbours exactly, so that a heatmap shows no slivers between its cells. A transposed
// or mirrored grid would draw every value at another cell's place while looking just as plausible.
#[test]
fn each_unit_cell_is_drawn_centred_on_its_indices_and_seamless_cells_tile_the_grid() {
    let (scene, ax, cells) = distinct_grid(|_, _| {});
    let drawn = drawn(&scene, ax, cells);
    for j in 0..2 {
        for i in 0..3 {
            let d = filled_in(&drawn, DISTINCT[3 * j + i]);
            let (x, y) = (i as f64, j as f64);
            assert_span(d.x, [x - 0.5, x + 0.5], &format!("cell ({j}, {i}) along x"));
            assert_span(d.y, [y - 0.5, y + 0.5], &format!("cell ({j}, {i}) along y"));
        }
    }
}

// Why: explicit edges are how a two-dimensional histogram of unequal bins, or a heatmap on measured coordinates, is
// drawn: each cell must lie exactly between its own two column edges and its own two row edges, whatever their
// spacing, so that its area is its bin. Edges may decrease, as rows numbered from the top do; row 0 still lies
// between the first two row edges, so with rows 10, 5, 0 the first row of values is drawn at the top.
#[test]
fn explicit_edges_place_each_cell_between_its_own_edges_even_when_they_decrease() {
    let columns = [0.0, 1.0, 3.0, 7.0];
    let rows = [10.0, 5.0, 0.0];
    let (scene, ax, cells) = distinct_grid(|c, fx| {
        c.columns = CellEdges::Edges(fx.vector(&columns));
        c.rows = CellEdges::Edges(fx.vector(&rows));
    });
    let drawn = drawn(&scene, ax, cells);
    assert_eq!(drawn.len(), 6);
    for j in 0..2 {
        for i in 0..3 {
            let d = filled_in(&drawn, DISTINCT[3 * j + i]);
            assert_span(
                d.x,
                sorted2(columns[i], columns[i + 1]),
                &format!("cell ({j}, {i}) between its column edges"),
            );
            assert_span(
                d.y,
                sorted2(rows[j], rows[j + 1]),
                &format!("cell ({j}, {i}) between its row edges"),
            );
        }
    }
    let (_, ymap) = axis_maps(&scene, ax);
    assert!(
        filled_in(&drawn, DISTINCT[0]).page.y < filled_in(&drawn, DISTINCT[3]).page.y,
        "row 0, between 10 and 5, is drawn above row 1 on an upward y axis: {ymap:?}"
    );
}

// Why: the gap is what sets the cells of an occupancy matrix apart, so that each reads as one thing; it is a fraction
// of each cell's extent shared equally by its two sides, along x and along y independently. At a gap of 0.2 across
// and 0.5 down, unit cells are drawn 0.8 wide and 0.5 high about their centres, and neighbours stand 0.2 and 0.5
// apart.
#[test]
fn a_gap_insets_each_cell_equally_on_both_sides_along_each_axis() {
    let (scene, ax, cells) = distinct_grid(|c, _| c.gap = CellGap { x: 0.2, y: 0.5 });
    let drawn = drawn(&scene, ax, cells);
    for j in 0..2 {
        for i in 0..3 {
            let d = filled_in(&drawn, DISTINCT[3 * j + i]);
            let (x, y) = (i as f64, j as f64);
            assert_span(d.x, [x - 0.4, x + 0.4], &format!("cell ({j}, {i}) along x"));
            assert_span(
                d.y,
                [y - 0.25, y + 0.25],
                &format!("cell ({j}, {i}) along y"),
            );
        }
    }
    let (first, second) = (
        filled_in(&drawn, DISTINCT[0]),
        filled_in(&drawn, DISTINCT[1]),
    );
    assert_close(second.x[0] - first.x[1], 0.2, TOL);
    let above = filled_in(&drawn, DISTINCT[3]);
    assert_close(above.y[0] - first.y[1], 0.5, TOL);
}

// Why: the gap is measured on the page (as the schema says), so on a logarithmic axis each cell is inset by the same
// distance on the page on both sides and looks centred in its slot. A gap applied in data units would leave far more
// space on one side than on the other, and the cells of a histogram over decades would look shifted. At a gap of
// 0.5 each side is inset by a quarter of the cell's page extent, which in data units is not symmetric at all.
#[test]
fn on_logarithmic_axes_the_gap_is_symmetric_on_the_page() {
    let columns = [1.0, 10.0, 1000.0];
    let rows = [1.0, 100.0];
    let (mut fx, ax) = axes();
    fx.ax(ax).x.scale = Scale::Log;
    fx.ax(ax).y.scale = Scale::Log;
    let scale = fx.color_scale(palette(&DISTINCT));
    let cells = fx.cells(ax, vec![1, 2], vec![0.0, 1.0], Some(scale), |c, fx| {
        c.columns = CellEdges::Edges(fx.vector(&columns));
        c.rows = CellEdges::Edges(fx.vector(&rows));
        c.gap = CellGap { x: 0.5, y: 0.5 };
    });
    let scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&scene, ax);
    let drawn = drawn(&scene, ax, cells);
    for i in 0..2 {
        let d = filled_in(&drawn, DISTINCT[i]);
        let (lo, hi) = (xmap.to_figure(columns[i]), xmap.to_figure(columns[i + 1]));
        let quarter = 0.25 * (hi - lo);
        assert_close(d.page.x, lo + quarter, 1e-6);
        assert_close(d.page.right(), hi - quarter, 1e-6);
        let (left, right) = (d.x[0] - columns[i], columns[i + 1] - d.x[1]);
        assert!(
            (left - right).abs() > 0.1,
            "precondition: in data units the gap is not symmetric ({left} against {right})"
        );
        let (top, bottom) = (ymap.to_figure(rows[1]), ymap.to_figure(rows[0]));
        let quarter = 0.25 * (bottom - top);
        assert_close(d.page.y, top + quarter, 1e-6);
        assert_close(d.page.bottom(), bottom - quarter, 1e-6);
    }
}

// Why: a reversed y axis is how a matrix is shown with its first row at the top, which is how an occupancy matrix is
// read (`ycategories` reverses y for exactly that), and a reversed x axis is as common. A cell computed on the page
// with the assumption that the page coordinate grows with the data would turn the inset of the gap into an outset,
// or draw a rectangle of negative extent; mapped back to data, the cells must have exactly the spans they have on
// unreversed axes, and the page order shows that the axes really are reversed.
#[test]
fn on_reversed_axes_cells_keep_their_spans_and_the_gap_stays_an_inset() {
    let (scene, ax, cells) = distinct_grid(|c, _| c.gap = CellGap { x: 0.2, y: 0.2 });
    let (mut fx, ax_reversed) = axes();
    fx.ax(ax_reversed).x.reverse = true;
    fx.ax(ax_reversed).y.reverse = true;
    let scale = fx.color_scale(palette(&DISTINCT));
    let reversed = fx.cells(
        ax_reversed,
        vec![2, 3],
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        Some(scale),
        |c, _| c.gap = CellGap { x: 0.2, y: 0.2 },
    );
    let reversed_scene = compile_figure(&fx.build());
    let (xmap, ymap) = axis_maps(&reversed_scene, ax_reversed);
    assert!(
        xmap.to_figure(0.0) > xmap.to_figure(1.0) && ymap.to_figure(0.0) < ymap.to_figure(1.0),
        "precondition: both axes run backwards on the page: {xmap:?} {ymap:?}"
    );
    let upright = drawn(&scene, ax, cells);
    let backwards = drawn(&reversed_scene, ax_reversed, reversed);
    assert_eq!(backwards.len(), 6);
    for colour in DISTINCT {
        let (u, b) = (filled_in(&upright, colour), filled_in(&backwards, colour));
        assert_span(b.x, u.x, &format!("the cell in {colour:?} along x"));
        assert_span(b.y, u.y, &format!("the cell in {colour:?} along y"));
        assert!(b.page.width > 0.0 && b.page.height > 0.0);
    }
    let (first, last) = (
        filled_in(&backwards, DISTINCT[0]),
        filled_in(&backwards, DISTINCT[5]),
    );
    assert!(
        first.page.x > last.page.x && first.page.y < last.page.y,
        "cell (0, 0) is drawn at the top right of reversed axes"
    );
}

// Why: a logarithmic axis has no place for an edge that is not positive, so a cell bounded by one cannot be drawn,
// and validation warns of it; the cells beside it must still be drawn exactly between their own edges, so that one
// unplaceable bin does not cost the reader the rest of the histogram. Unit edges start at −0.5, so on a logarithmic
// x axis the first column of unit cells is the one left out.
#[test]
fn on_a_logarithmic_axis_a_cell_bounded_by_an_edge_that_is_not_positive_is_not_drawn() {
    let columns = [-1.0, 1.0, 10.0, 100.0];
    let (mut fx, ax) = axes();
    fx.ax(ax).x.scale = Scale::Log;
    let scale = fx.color_scale(palette(&DISTINCT));
    let cells = fx.cells(ax, vec![1, 3], vec![0.0, 1.0, 2.0], Some(scale), |c, fx| {
        c.columns = CellEdges::Edges(fx.vector(&columns));
    });
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    assert_eq!(
        drawn_cells.len(),
        2,
        "the first column is left out: {drawn_cells:#?}"
    );
    assert!(
        !drawn_cells
            .iter()
            .any(|d| d.fill.is_some_and(|f| rgb8(f) == rgb8_of(DISTINCT[0]))),
        "the cell between −1 and 1 is not drawn"
    );
    assert_span(
        filled_in(&drawn_cells, DISTINCT[1]).x,
        [1.0, 10.0],
        "the second column",
    );
    assert_span(
        filled_in(&drawn_cells, DISTINCT[2]).x,
        [10.0, 100.0],
        "the third column",
    );

    let (mut fx, ax) = axes();
    fx.ax(ax).x.scale = Scale::Log;
    let scale = fx.color_scale(palette(&DISTINCT));
    let cells = fx.cells(ax, vec![1, 3], vec![0.0, 1.0, 2.0], Some(scale), |_, _| {});
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    assert_eq!(
        drawn_cells.len(),
        2,
        "unit cells lose their first column: {drawn_cells:#?}"
    );
    assert_span(
        filled_in(&drawn_cells, DISTINCT[1]).x,
        [0.5, 1.5],
        "unit column 1",
    );
    assert_span(
        filled_in(&drawn_cells, DISTINCT[2]).x,
        [1.5, 2.5],
        "unit column 2",
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------------------------------------------

// Why: the colour of a cell is its value, so it must be exactly the colour the scale gives that value: through a
// continuous scale, the colormap at the value's place between the limits. Red to blue between 0 and 1 is linear in
// sRGB, so 0.5 is an even purple; the named colormap of the default scale must agree with its own table entry.
#[test]
fn through_a_continuous_scale_each_cell_takes_the_colour_of_its_value() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    let cells = fx.cells(ax, vec![1, 3], vec![0.0, 0.5, 1.0], Some(scale), |_, _| {});
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    for (i, t) in [0.0, 0.5, 1.0].into_iter().enumerate() {
        assert_colour(
            at(&drawn_cells, i as f64, 0.0).fill,
            red_to_blue_at(t),
            2,
            &format!("the cell holding {t}"),
        );
    }

    let (mut fx, ax) = axes();
    let scale = fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        limits: Limits::Manual { min: 0.0, max: 8.0 },
        ..ContinuousColorScale::default()
    }));
    let cells = fx.cells(ax, vec![1, 3], vec![0.0, 2.0, 8.0], Some(scale), |_, _| {});
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    for (i, entry) in [0usize, 64, 255].into_iter().enumerate() {
        let fill = at(&drawn_cells, i as f64, 0.0).fill.expect("filled");
        let got = nearest_lut_index(&VIRIDIS, fill);
        assert!(
            got.abs_diff(entry) <= 1,
            "cell {i}: viridis entry {got}, expected {entry}"
        );
    }
}

// Why: the case matrix of #74 is coloured through a stepped colourmap, so that a reader can match a cell to a band
// of the colourbar: every value of a band must take the band's one colour, and a value exactly on an edge between
// two bands belongs to the upper band. Three bands of red to blue are red, an even purple and blue.
#[test]
fn through_a_discrete_scale_each_cell_takes_the_single_colour_of_its_band() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(ColorScaleKind::Discrete(DiscreteColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, BLUE],
        },
        levels: ColorScaleLevels::Explicit {
            values: vec![0.0, 1.0, 2.0, 3.0],
        },
        ..DiscreteColorScale::default()
    }));
    let cells = fx.cells(
        ax,
        vec![1, 4],
        vec![0.5, 1.9, 2.0, 2.5],
        Some(scale),
        |_, _| {},
    );
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    for (i, expected, what) in [
        (0, red_to_blue_at(0.0), "0.5 lies in the first band"),
        (1, red_to_blue_at(0.5), "1.9 lies in the middle band"),
        (
            2,
            red_to_blue_at(1.0),
            "2 lies on an edge and belongs to the upper band",
        ),
        (3, red_to_blue_at(1.0), "2.5 lies in the last band"),
    ] {
        assert_colour(at(&drawn_cells, i as f64, 0.0).fill, expected, 1, what);
    }
}

// Why: an occupancy matrix stores a status per cell as a whole number naming a category, often as bytes; each cell
// must take exactly the colour of the entry its value names, on an indexed and on a categorical scale and whether
// the values are floating-point or 8-bit, or a status would be shown as another.
#[test]
fn through_an_indexed_or_categorical_scale_each_cell_takes_the_entry_its_value_names() {
    for kind in [palette(&[RED, GREEN, BLUE]), statuses(&[RED, GREEN, BLUE])] {
        for bytes in [false, true] {
            let (mut fx, ax) = axes();
            let scale = fx.color_scale(kind.clone());
            let cells = if bytes {
                fx.cells(ax, vec![1, 4], vec![2u8, 0, 1, 2], Some(scale), |_, _| {})
            } else {
                fx.cells(
                    ax,
                    vec![1, 4],
                    vec![2.0, 0.0, 1.0, 2.0],
                    Some(scale),
                    |_, _| {},
                )
            };
            let scene = compile_figure(&fx.build());
            assert!(
                warnings_naming(&scene, cells).is_empty(),
                "{:?}",
                scene.warnings
            );
            let drawn_cells = drawn(&scene, ax, cells);
            for (i, colour) in [BLUE, RED, GREEN, BLUE].into_iter().enumerate() {
                assert_colour(
                    at(&drawn_cells, i as f64, 0.0).fill,
                    rgb8_of(colour),
                    0,
                    &format!("{kind:?}, bytes {bytes}: cell {i}"),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Values the scale cannot place
// ---------------------------------------------------------------------------------------------------------------

// Why: the rows of an occupancy matrix are ragged (a subroutine has as many cells as it has branches), so they are
// padded with NaN to the longest row, and the padding must draw nothing at all: no face and no outline, even when
// the cells are outlined, or the reader would see empty boxes for branches that do not exist. NaN is transparent by
// default on every kind of scale, and the cells that hold a status are drawn as usual around the padding.
#[test]
fn the_nan_padding_of_ragged_rows_draws_nothing_even_when_cells_are_outlined() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(statuses(&[RED, GREEN, BLUE]));
    let nan = f64::NAN;
    let cells = fx.cells(
        ax,
        vec![2, 3],
        vec![0.0, 1.0, 2.0, 1.0, nan, nan],
        Some(scale),
        |c, _| {
            c.gap = CellGap { x: 0.2, y: 0.2 };
            c.edge.color = ColorSpec::Rgba { color: BLACK };
        },
    );
    let scene = compile_figure(&fx.build());
    assert!(
        warnings_naming(&scene, cells).is_empty(),
        "{:?}",
        scene.warnings
    );
    let drawn_cells = drawn(&scene, ax, cells);
    assert_eq!(
        drawn_cells.len(),
        4,
        "only the cells that hold a status: {drawn_cells:#?}"
    );
    for (x, y) in [(1.0, 1.0), (2.0, 1.0)] {
        assert!(
            !covers(&drawn_cells, x, y),
            "the padding at ({x}, {y}) draws nothing"
        );
    }
    for (x, y, colour) in [
        (0.0, 0.0, RED),
        (1.0, 0.0, GREEN),
        (2.0, 0.0, BLUE),
        (0.0, 1.0, GREEN),
    ] {
        let d = at(&drawn_cells, x, y);
        assert_colour(
            d.fill,
            rgb8_of(colour),
            0,
            &format!("the cell at ({x}, {y})"),
        );
        assert!(d.stroke.is_some(), "the cell at ({x}, {y}) is outlined");
    }
}

// Why: a value the scale cannot place follows the scale's out-of-range policy, as it does for a scalar image: a fixed
// colour draws the cell in that colour (the neutral grey "not entered" of the case matrix of #74, which its key
// explains), a clamp draws the nearest end colour, and a transparent policy draws nothing. Each cell beside them
// keeps its own colour.
#[test]
fn a_value_the_scale_cannot_place_follows_the_policy_of_its_category() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    let cells = fx.cells(
        ax,
        vec![1, 5],
        vec![-1.0, 2.0, f64::NAN, 0.5, -3.0],
        Some(scale),
        |_, _| {},
    );
    let s = fx.scale_mut(scale);
    s.non_finite = out_of_range(OutOfRange::Rgba { color: GREY });
    if let ColorScaleKind::Continuous(c) = &mut s.kind {
        c.below = out_of_range(OutOfRange::Clamp);
        c.above = out_of_range(OutOfRange::Rgba { color: GREEN });
    }
    let scene = compile_figure(&fx.build());
    assert!(
        warnings_naming(&scene, cells).is_empty(),
        "{:?}",
        scene.warnings
    );
    let drawn_cells = drawn(&scene, ax, cells);
    let fill = |i: usize| at(&drawn_cells, i as f64, 0.0).fill;
    assert_colour(
        fill(0),
        rgb8_of(RED),
        1,
        "−1 lies below and is clamped to the low end",
    );
    assert_colour(
        fill(1),
        rgb8_of(GREEN),
        0,
        "2 lies above and takes the fixed colour",
    );
    assert_colour(fill(2), rgb8_of(GREY), 0, "NaN takes the not-entered grey");
    assert_colour(fill(3), red_to_blue_at(0.5), 2, "0.5 keeps its own colour");

    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    let cells = fx.cells(ax, vec![1, 2], vec![-1.0, 0.5], Some(scale), |_, _| {});
    if let ColorScaleKind::Continuous(c) = &mut fx.scale_mut(scale).kind {
        c.below = out_of_range(OutOfRange::Transparent);
    }
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    assert!(
        !covers(&drawn_cells, 0.0, 0.0),
        "a transparent policy draws nothing"
    );
    assert!(
        covers(&drawn_cells, 1.0, 0.0),
        "the cell beside it is drawn"
    );
}

// Why: a strict policy says that a value the scale cannot place is a mistake in the data; validation refuses it, and
// the compiler, which never fails, leaves the cells out with one warning naming them, exactly as it leaves out a
// scalar image, rather than draw a misleading matrix. The rest of the figure is drawn, and strict cells with nothing
// to offend are drawn as if the policy were lenient.
#[test]
fn a_strict_policy_with_an_offending_value_leaves_the_cells_out_with_one_warning_as_for_a_scalar_image()
 {
    let (mut fx, ax) = axes();
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    let scale = fx.color_scale(statuses(&[RED, GREEN]));
    let offending = fx.cells(ax, vec![1, 2], vec![0.0, 2.0], Some(scale), |_, _| {});
    let scene = compile_figure(&fx.build());
    assert!(
        from_source(&leaves(&scene), offending).is_empty(),
        "cells naming no category under a strict policy are not drawn"
    );
    let warnings = warnings_naming(&scene, offending);
    assert_eq!(
        warnings.len(),
        1,
        "one warning names the cells: {:?}",
        scene.warnings
    );
    // The warning must give the reason a reader can act on (the strict policy of the scale), as it does for a scalar
    // image, rather than any other reason for leaving the cells out.
    assert!(
        warnings[0].contains("strict"),
        "the warning names the strict policy: {warnings:?}"
    );
    assert!(
        !from_source(&leaves(&scene), line).is_empty(),
        "the line is still drawn"
    );

    let (mut fx, ax) = axes();
    let scale = fx.color_scale(statuses(&[RED, GREEN]));
    fx.scale_mut(scale).non_finite = out_of_range(OutOfRange::Strict);
    let clean = fx.cells(ax, vec![1, 2], vec![0.0, 1.0], Some(scale), |_, _| {});
    let scene = compile_figure(&fx.build());
    assert!(
        warnings_naming(&scene, clean).is_empty(),
        "{:?}",
        scene.warnings
    );
    assert_eq!(
        drawn(&scene, ax, clean).len(),
        2,
        "strict cells with nothing to offend are drawn"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Alpha, opacity and outlines
// ---------------------------------------------------------------------------------------------------------------

// Why: the alpha of each cell is how a matrix shows how far each value can be trusted, and the face opacity fades the
// cells as a whole, as for a scalar image: the alpha multiplies the alpha of the cell's colour and the opacity
// multiplies both, a NaN alpha or an alpha of 0 draws nothing, an alpha outside [0, 1] is clamped, and an 8-bit
// alpha is a fraction of 255. A fixed out-of-range colour stands in for the cell's colour and is faded the same way.
#[test]
fn the_alpha_of_each_cell_multiplies_its_colour_and_the_face_opacity() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    fx.scale_mut(scale).non_finite = out_of_range(OutOfRange::Rgba {
        color: Color::rgba(0.5, 0.5, 0.5, 0.8),
    });
    let cells = fx.cells(
        ax,
        vec![1, 5],
        vec![0.5, 0.5, 0.5, 0.5, f64::NAN],
        Some(scale),
        |c, fx| {
            c.alpha = Some(fx.matrix(1, 5, vec![0.5, f64::NAN, 2.0, -1.0, 0.5]));
            c.face_opacity = 0.5;
        },
    );
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    let alpha = |i: usize| f64::from(at(&drawn_cells, i as f64, 0.0).fill.expect("filled").a);
    assert_close(alpha(0), 0.25, 1e-3);
    assert!(!covers(&drawn_cells, 1.0, 0.0), "a NaN alpha draws nothing");
    assert_close(alpha(2), 0.5, 1e-3);
    assert!(
        !covers(&drawn_cells, 3.0, 0.0),
        "an alpha of −1 counts as 0 and draws nothing"
    );
    assert_close(alpha(4), 0.8 * 0.5 * 0.5, 1e-3);

    let (mut fx, ax) = axes();
    let scale = fx.color_scale(palette(&[RED]));
    let cells = fx.cells(ax, vec![1, 2], vec![0u8, 0], Some(scale), |c, fx| {
        c.alpha = Some(fx.bytes(vec![1, 2], vec![51, 255]));
    });
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    assert_close(
        f64::from(at(&drawn_cells, 0.0, 0.0).fill.unwrap().a),
        0.2,
        1e-3,
    );
    assert_close(
        f64::from(at(&drawn_cells, 1.0, 0.0).fill.unwrap().a),
        1.0,
        1e-3,
    );
}

// Why: cells are seamless and unoutlined by default, as a heatmap is; an outline of a fixed colour sets every cell
// off in that colour and width, faded by the edge opacity alone, so that a grid of white lines can be drawn over a
// matrix without touching the faces.
#[test]
fn cells_have_no_outline_by_default_and_a_fixed_edge_outlines_every_cell() {
    let (scene, ax, cells) = distinct_grid(|_, _| {});
    assert!(
        drawn(&scene, ax, cells).iter().all(|d| d.stroke.is_none()),
        "no outline by default"
    );

    let (scene, ax, cells) = distinct_grid(|c, _| {
        c.edge.color = ColorSpec::Rgba { color: BLACK };
        c.edge.width_pt = 1.5;
        c.edge_opacity = 0.5;
        c.face_opacity = 0.75;
    });
    let drawn_cells = drawn(&scene, ax, cells);
    assert_eq!(drawn_cells.len(), 6);
    for d in &drawn_cells {
        let stroke = d.stroke.as_ref().expect("every cell is outlined");
        assert_colour(Some(stroke.color), [0, 0, 0], 0, "a black outline");
        assert_close(stroke.width, 1.5, 1e-12);
        assert_close(f64::from(stroke.color.a), 0.5, 1e-6);
        assert_close(f64::from(d.fill.expect("filled").a), 0.75, 1e-6);
    }
}

// Why: a colormapped outline gives each outline the colour of its own cell, as a colormapped bar outline does, so
// that an outlined matrix reads as one colour per cell rather than as faces framed in a colour of their own.
#[test]
fn a_colormapped_edge_gives_each_outline_the_colour_of_its_cell() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    let cells = fx.cells(ax, vec![1, 2], vec![0.0, 1.0], Some(scale), |c, _| {
        c.edge.color = ColorSpec::Colormapped;
        c.edge.width_pt = 1.0;
    });
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    assert_colour(
        at(&drawn_cells, 0.0, 0.0).stroke.as_ref().map(|s| s.color),
        rgb8_of(RED),
        1,
        "the outline of the cell holding 0",
    );
    assert_colour(
        at(&drawn_cells, 1.0, 0.0).stroke.as_ref().map(|s| s.color),
        rgb8_of(BLUE),
        1,
        "the outline of the cell holding 1",
    );
}

// Why: an automatic outline colour follows the colour of what it outlines, as the automatic edge colour of a surface
// follows the colour of each face; for cells that is the cell's own colour, so that `auto` and `colormapped` agree
// and an outlined matrix still reads as one colour per cell rather than as faces framed in a colour of the order.
#[test]
fn an_automatic_edge_gives_each_outline_the_colour_of_its_cell() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    let cells = fx.cells(ax, vec![1, 2], vec![0.0, 1.0], Some(scale), |c, _| {
        c.edge.color = ColorSpec::Auto;
        c.edge.width_pt = 1.0;
    });
    let scene = compile_figure(&fx.build());
    let drawn_cells = drawn(&scene, ax, cells);
    assert_colour(
        at(&drawn_cells, 0.0, 0.0).stroke.as_ref().map(|s| s.color),
        rgb8_of(RED),
        1,
        "the automatic outline of the cell holding 0",
    );
    assert_colour(
        at(&drawn_cells, 1.0, 0.0).stroke.as_ref().map(|s| s.color),
        rgb8_of(BLUE),
        1,
        "the automatic outline of the cell holding 1",
    );
}

// Why: hiding the cells through their legend entry or the property editor must remove them from the page.
#[test]
fn hidden_cells_draw_nothing() {
    let (scene, ax, cells) = distinct_grid(|c, _| c.visible = false);
    assert!(drawn(&scene, ax, cells).is_empty());
}

// ---------------------------------------------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------------------------------------------

// Why: cells fill their extent as the nodes of a grid fill theirs, so the automatic limits end exactly at the outer
// edges of the cells, rather than at the next tick value, which would leave an empty margin on one side of the matrix
// and not the other. Unit cells of three columns and two rows end at −0.5 and 2.5 along x and −0.5 and 1.5 along y;
// explicit edges end at their extremes, whichever way they run.
#[test]
fn automatic_limits_end_exactly_at_the_outer_edges_of_the_cells() {
    let (scene, ax, _) = distinct_grid(|_, _| {});
    let (xmap, ymap) = axis_maps(&scene, ax);
    assert_close(xmap.min, -0.5, TOL);
    assert_close(xmap.max, 2.5, TOL);
    assert_close(ymap.min, -0.5, TOL);
    assert_close(ymap.max, 1.5, TOL);

    let (scene, ax, _) = distinct_grid(|c, fx| {
        c.columns = CellEdges::Edges(fx.vector(&[0.0, 1.0, 3.0, 7.0]));
        c.rows = CellEdges::Edges(fx.vector(&[10.0, 5.0, 0.0]));
    });
    let (xmap, ymap) = axis_maps(&scene, ax);
    assert_close(xmap.min, 0.0, TOL);
    assert_close(xmap.max, 7.0, TOL);
    assert_close(ymap.min, 0.0, TOL);
    assert_close(ymap.max, 10.0, TOL);
}

// ---------------------------------------------------------------------------------------------------------------
// Robustness and export
// ---------------------------------------------------------------------------------------------------------------

// Why: the PDF exporter decides whether to rasterise content from the number of pieces it would otherwise draw as
// vectors, so cells are counted as surface faces are, by the number drawn: a cell left transparent is not drawn and
// must not be counted. A two-dimensional histogram of 100 × 100 bins is an ordinary figure, and every one of its ten
// thousand cells must be drawn and counted.
#[test]
fn cells_are_counted_as_dense_content_by_the_number_drawn() {
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    let cells = fx.cells(
        ax,
        vec![2, 3],
        vec![0.0, 0.5, f64::NAN, 1.0, 0.2, 0.4],
        Some(scale),
        |_, _| {},
    );
    let scene = compile_figure(&fx.build());
    assert_eq!(dense_groups(&scene), vec![(Some(cells), 5)]);

    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    let values: Vec<f64> = (0..100 * 100).map(|k| (k % 97) as f64 / 96.0).collect();
    let histogram = fx.cells(ax, vec![100, 100], values, Some(scale), |_, _| {});
    let scene = compile_figure(&fx.build());
    assert_eq!(dense_groups(&scene), vec![(Some(histogram), 10_000)]);
}

// Why: a two-dimensional histogram of 100 × 100 bins is an ordinary figure, and the viewer recompiles the scene on
// every edit, so cells must cost about what the faces of a surface of as many cells cost, rather than growing with
// the square of the number of cells (as comparing every cell with every other, or resolving the scale afresh for
// each cell by searching the figure, would). The bound is relative to a surface compiled in the same process, with
// slack, so that it holds on a slow machine and in a debug build while still catching work that is quadratic.
#[test]
fn a_hundred_by_hundred_grid_compiles_in_about_the_time_of_a_surface_of_as_many_faces() {
    use std::time::{Duration, Instant};

    let n = 100;
    let (mut fx, ax) = axes();
    let scale = fx.color_scale(red_to_blue());
    let values: Vec<f64> = (0..n * n).map(|k| (k % 97) as f64 / 96.0).collect();
    fx.cells(ax, vec![n, n], values, Some(scale), |_, _| {});
    let cells_figure = fx.build();

    let (mut fx, ax) = axes();
    let nodes: Vec<f64> = (0..=n).map(|k| k as f64).collect();
    fx.surface(ax, &nodes, &nodes, |x, y| ((x + y) % 97.0) / 96.0, |_| {});
    let surface_figure = fx.build();

    let time = |figure: &ironlab_ir::Figure| {
        let start = Instant::now();
        let scene = compile_figure(figure);
        (start.elapsed(), scene)
    };
    // Compile each once first, so that one-off costs such as loading the fonts are not counted against either.
    let _ = time(&surface_figure);
    let _ = time(&cells_figure);
    let (surface, _) = time(&surface_figure);
    let (cells, scene) = time(&cells_figure);
    assert_eq!(
        dense_groups(&scene).iter().map(|(_, k)| k).sum::<u64>(),
        10_000,
        "precondition: every cell is drawn"
    );
    assert!(
        cells <= surface * 10 + Duration::from_secs(2),
        "10 000 cells compiled in {cells:?}, a surface of 10 000 faces in {surface:?}"
    );
}

// Why: cells with no rows or no columns are valid (ADR 0012), as the empty result of a query is; they draw nothing
// and must not stop the rest of the axes from being drawn.
#[test]
fn cells_with_no_rows_or_no_columns_draw_nothing_and_the_axes_is_still_drawn() {
    for shape in [vec![0, 3], vec![2, 0], vec![0, 0]] {
        let (mut fx, ax) = axes();
        let scale = fx.color_scale(red_to_blue());
        let empty = fx.cells(ax, shape.clone(), Vec::<f64>::new(), Some(scale), |_, _| {});
        let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
        let scene = compile_figure(&fx.build());
        assert!(from_source(&leaves(&scene), empty).is_empty(), "{shape:?}");
        assert!(
            !from_source(&leaves(&scene), line).is_empty(),
            "{shape:?}: the line is drawn"
        );
    }
}

// Why: cells are valid only in a two-dimensional axes, as validation says; the compiler never fails, so cells that
// reach it in a three-dimensional axes are left out with one warning naming them, rather than drawn flat at an
// arbitrary height or crashing the scene, and the rest of the axes is drawn.
#[test]
fn cells_in_a_three_dimensional_axes_are_left_out_with_a_warning() {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let scale = fx.color_scale(red_to_blue());
    let cells = fx.cells(ax, vec![1, 2], vec![0.0, 1.0], Some(scale), |_, _| {});
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], Some(&[0.0, 1.0]), |_| {});
    let scene = compile_figure(&fx.build());
    assert!(
        from_source(&leaves(&scene), cells).is_empty(),
        "the cells are not drawn"
    );
    assert!(
        warnings_naming(&scene, cells)
            .iter()
            .any(|w| w.contains("three-dimensional")),
        "the cells are left out because the axes is three-dimensional, not because cells are not drawn: {:?}",
        scene.warnings
    );
    assert_eq!(
        warnings_naming(&scene, cells).len(),
        1,
        "one warning names the cells: {:?}",
        scene.warnings
    );
    assert!(
        !from_source(&leaves(&scene), line).is_empty(),
        "the line is drawn"
    );
}
