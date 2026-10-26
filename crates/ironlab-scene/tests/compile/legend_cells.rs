//! The legend sample of cells: what a key draws beside the label of cells.
//!
//! Cells map every value through a colour scale, as a scalar image does, so they are keyed exactly as a scalar image
//! is: by a sweep of their scale, faded by the opacity of their faces but not by their alpha, which varies over the
//! cells. Each test therefore draws a scalar image through the same scale beside the cells and compares the two
//! samples colour for colour, so that the expectation is the behaviour the reader already knows from images rather
//! than a second account of the sweep. The samples are found as the items of a legend panel drawn left of the label
//! of their entry, as in `legend_color_keys`.

use ironlab_ir::{
    CategoricalColorScale, Color, ColorCategory, ColorScaleKind, ColorScaleLevels, Colormap,
    ContinuousColorScale, DiscreteColorScale, LegendEntryColumns, LegendEntryLayout,
    LegendEntryOrder, Limits, NodeId, OutOfRange, Text,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{Point, Rect, Rgba};

use crate::common::{Fx, cell, compile_figure, out_of_range, rgb8, text, xy};
use crate::probe::{Leaf, from_source, leaves, pixel, runs_with_text, text_bbox, union};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 0.6, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

// ---------------------------------------------------------------------------------------------------------------
// Fixtures and probes
// ---------------------------------------------------------------------------------------------------------------

/// An axes above a legend panel of one column that lists the axes' entries, holding cells labelled "Cells" and a
/// scalar image labelled "Image", both mapped through one scale of `kind` and holding `values` (a 1 × n row), each
/// edited by its own closure. Returns the compiled scene and the panel.
fn keyed_beside_an_image(
    kind: ColorScaleKind,
    values: Vec<f64>,
    edit_cells: impl FnOnce(&mut ironlab_ir::Cells, &mut Fx),
    edit_image: impl FnOnce(&mut ironlab_ir::ScalarImage),
) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    let ax = fx.axes2d(0, 0);
    let panel = fx.panel(cell(1, 0, 1, 1));
    fx.legend_mut(panel).layout = LegendEntryLayout {
        columns: LegendEntryColumns::Count(1),
        order: LegendEntryOrder::ColumnMajor,
    };
    fx.legend_in(ax, panel);
    let scale = fx.color_scale(kind);
    let n = values.len();
    fx.cells(ax, vec![1, n], values.clone(), Some(scale), |c, fx| {
        c.label = text("Cells");
        edit_cells(c, fx);
    });
    fx.scalar_image(ax, vec![1, n], values, Some(scale), xy(None, None), |i| {
        i.label = text("Image");
        edit_image(i);
    });
    (compile_figure(&fx.build()), panel)
}

/// Returns the bounding box of the label `text`, which must be drawn exactly once.
#[track_caller]
fn label(scene: &Scene, text: &str) -> Rect {
    let all = leaves(scene);
    assert_eq!(
        runs_with_text(&all, text).len(),
        1,
        "the label {text:?} is drawn once"
    );
    text_bbox(&all, text).expect("the label has a box")
}

/// Returns the items of the legend `source` drawn for the sample beside the label `text`: those that end left of
/// the label, start no further left than four label heights from it, and whose vertical centre lies within the
/// height of the label.
fn sample_beside(scene: &Scene, source: NodeId, text: &str) -> Vec<Leaf> {
    let label = label(scene, text);
    from_source(&leaves(scene), source)
        .into_iter()
        .filter(|leaf| leaf.glyphs().is_none())
        .filter(|leaf| {
            leaf.visible_bbox().is_some_and(|b| {
                let cy = b.y + b.height / 2.0;
                b.right() <= label.x + 1e-6
                    && b.x >= label.x - 4.0 * label.height.max(9.0)
                    && cy >= label.y
                    && cy <= label.bottom()
            })
        })
        .collect()
}

/// Returns the paint of the top-most image or filled path under `p`, as a colour with its alpha.
fn fill_under(leaves: &[Leaf], p: Point) -> Option<Rgba> {
    leaves.iter().rev().find_map(|l| {
        if let Some(image) = l.image() {
            let b = l.bbox()?;
            if !b.contains(p) {
                return None;
            }
            let local = l.transform.inverse()?.apply(p);
            let fx = (local.x - image.rect.x) / image.rect.width;
            let fy = (local.y - image.rect.y) / image.rect.height;
            let column = ((fx * f64::from(image.width)).floor().max(0.0) as usize)
                .min(image.width as usize - 1);
            let row = ((fy * f64::from(image.height)).floor().max(0.0) as usize)
                .min(image.height as usize - 1);
            let px = pixel(image, row, column);
            return Some(Rgba::new(
                f32::from(px[0]) / 255.0,
                f32::from(px[1]) / 255.0,
                f32::from(px[2]) / 255.0,
                f32::from(px[3]) / 255.0,
            ));
        }
        let fill = l.path()?.fill?;
        l.bbox().filter(|b| b.contains(p)).map(|_| fill.color)
    })
}

/// Returns the fill of the sample beside the label `text` at the fraction `f` across it, on its centre line.
#[track_caller]
fn sample_fill(scene: &Scene, source: NodeId, text: &str, f: f64) -> Rgba {
    let sample = sample_beside(scene, source, text);
    let b = sample
        .iter()
        .filter_map(Leaf::bbox)
        .reduce(union)
        .unwrap_or_else(|| panic!("a sample is drawn beside {text:?}"));
    let p = Point::new(b.x + f * b.width, b.y + b.height / 2.0);
    fill_under(&sample, p)
        .unwrap_or_else(|| panic!("the sample beside {text:?} is filled at {f} across it"))
}

/// Asserts that the samples of the cells and of the image are painted alike at each of `fractions` across them,
/// colour and alpha, to one 8-bit level.
#[track_caller]
fn assert_keyed_alike(scene: &Scene, panel: NodeId, fractions: &[f64], what: &str) {
    for &f in fractions {
        let (cells, image) = (
            sample_fill(scene, panel, "Cells", f),
            sample_fill(scene, panel, "Image", f),
        );
        let close = rgb8(cells)
            .iter()
            .zip(rgb8(image))
            .all(|(a, b)| a.abs_diff(b) <= 1)
            && (cells.a - image.a).abs() <= 1.0 / 255.0 + 1e-6;
        assert!(
            close,
            "{what}: at {f} across, the cells are keyed in {cells:?} and the image in {image:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------------------------

// Why: the key of cells must tell the reader what their colours mean, which for any colour-mapped artist is the whole
// of its scale; an empty patch (the stopgap while cells were not drawn) or one middle colour tells the reader
// nothing. Cells are keyed exactly as a scalar image through the same scale is, whatever the kind of the scale: a
// gradient from the low end at the left to the high end at the right for a continuous scale, a row of bands for a
// discrete one, and the entries side by side for a categorical one.
#[test]
fn the_sample_of_cells_is_the_sweep_of_their_scale_as_for_a_scalar_image() {
    let continuous = ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, BLUE],
        },
        limits: Limits::Manual { min: 0.0, max: 1.0 },
        ..ContinuousColorScale::default()
    });
    let (scene, panel) = keyed_beside_an_image(continuous, vec![0.2, 0.8], |_, _| {}, |_| {});
    let (left, right) = (
        sample_fill(&scene, panel, "Cells", 0.08),
        sample_fill(&scene, panel, "Cells", 0.92),
    );
    assert_ne!(
        rgb8(left),
        rgb8(right),
        "the sample shows more than one colour"
    );
    assert_keyed_alike(&scene, panel, &[0.08, 0.3, 0.5, 0.7, 0.92], "continuous");

    let banded = ColorScaleKind::Discrete(DiscreteColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, BLUE],
        },
        levels: ColorScaleLevels::Explicit {
            values: vec![0.0, 1.0, 2.0, 3.0],
        },
        ..DiscreteColorScale::default()
    });
    let (scene, panel) = keyed_beside_an_image(banded, vec![0.5, 2.5], |_, _| {}, |_| {});
    assert_keyed_alike(&scene, panel, &[1.0 / 6.0, 0.5, 5.0 / 6.0], "discrete");

    let categorical = ColorScaleKind::Categorical(CategoricalColorScale {
        categories: [(RED, "taken"), (GREEN, "not taken"), (BLUE, "unreachable")]
            .into_iter()
            .map(|(color, label)| ColorCategory {
                color,
                label: Text::plain(label),
            })
            .collect(),
        outside: out_of_range(OutOfRange::Strict),
    });
    let (scene, panel) = keyed_beside_an_image(categorical, vec![0.0, 1.0, 2.0], |_, _| {}, |_| {});
    assert_keyed_alike(&scene, panel, &[1.0 / 6.0, 0.5, 5.0 / 6.0], "categorical");
}

// Why: the key should look like what it names, so cells whose faces are faded are keyed by a faded sweep, as an
// image of the same opacity is; their alpha varies from cell to cell and has no single value to show, so it does not
// carry into the key, as the alpha of an image does not.
#[test]
fn the_face_opacity_of_cells_carries_into_their_sample_but_their_alpha_does_not() {
    let kind = ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, BLUE],
        },
        limits: Limits::Manual { min: 0.0, max: 1.0 },
        ..ContinuousColorScale::default()
    });
    let (scene, panel) = keyed_beside_an_image(
        kind,
        vec![0.2, 0.8],
        |c, fx| {
            c.face_opacity = 0.5;
            c.alpha = Some(fx.matrix(1, 2, vec![0.2, 0.2]));
        },
        |i| i.opacity = 0.5,
    );
    let faded = sample_fill(&scene, panel, "Cells", 0.5);
    assert!(
        (f64::from(faded.a) - 0.5).abs() <= 2.0 / 255.0,
        "the sample takes the face opacity of 0.5 and not the alpha of 0.2: {faded:?}"
    );
    assert_keyed_alike(&scene, panel, &[0.1, 0.5, 0.9], "faded");
}
