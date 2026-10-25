//! The legend samples of bars and stairs: what a key draws beside the label of a bar series or of stairs, and how
//! their entries merge (ADR 0018).
//!
//! A sample is only useful if it looks like what it names, so each test compares the colours a sample is painted in
//! with those the artist itself is drawn in. The samples are found as the items of a legend panel drawn left of the
//! label of their entry, as in `legend_color_keys`.

use ironlab_ir::{
    Color, ColorScaleKind, ColorSpec, Colormap, ContinuousColorScale, LegendEntryColumns,
    LegendEntryLayout, LegendEntryOrder, Limits, NodeId, Text,
};
use ironlab_scene::display::{Point, Rect, Rgba};
use ironlab_scene::{Scene, SceneWarningKind};

use crate::common::{COLOUR_ORDER, Fx, cell, compile_figure, rgb8, rgb8_close, rgba, text};
use crate::probe::{Leaf, from_source, leaves, panel_entries, pixel, runs_with_text, text_bbox};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const GREEN: Color = Color::rgb(0.0, 0.6, 0.0);

// ---------------------------------------------------------------------------------------------------------------
// Fixtures and probes
// ---------------------------------------------------------------------------------------------------------------

/// An axes above a legend panel of one column that lists the axes' entries, holding what `add` adds. Returns the
/// compiled scene and the panel.
fn keyed(add: impl FnOnce(&mut Fx, NodeId)) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    let ax = fx.axes2d(0, 0);
    let panel = fx.panel(cell(1, 0, 1, 1));
    fx.legend_mut(panel).layout = LegendEntryLayout {
        columns: LegendEntryColumns::Count(1),
        order: LegendEntryOrder::ColumnMajor,
    };
    fx.legend_in(ax, panel);
    add(&mut fx, ax);
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

/// The fills of the paths of a sample.
fn fills(sample: &[Leaf]) -> Vec<Rgba> {
    sample
        .iter()
        .filter_map(|l| l.path()?.fill)
        .map(|f| f.color)
        .collect()
}

/// The stroke colours of the paths of a sample.
fn strokes(sample: &[Leaf]) -> Vec<Rgba> {
    sample
        .iter()
        .filter_map(|l| l.path()?.stroke.as_ref())
        .map(|s| s.color)
        .collect()
}

fn is(colour: Rgba, expected: [u8; 3]) -> bool {
    rgb8_close(rgb8(colour), expected)
}

fn rgb8_of(color: Color) -> [u8; 3] {
    rgb8(rgba(color))
}

/// Returns the colour painted at `p` by the top-most filled path or image of a sample.
fn fill_under(sample: &[Leaf], p: Point) -> Option<Rgba> {
    sample.iter().rev().find_map(|l| {
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

fn distance(a: [u8; 3], b: [u8; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (f64::from(*x) - f64::from(y)).powi(2))
        .sum::<f64>()
        .sqrt()
}

// ---------------------------------------------------------------------------------------------------------------
// Bars
// ---------------------------------------------------------------------------------------------------------------

// Why: a bar series is keyed by a small bar: a swatch filled with its face and outlined by its edge, so that the
// reader matches the entry to the bars by the same fill and the same outline. An automatic face is keyed in the
// colour the bars took from the colour order, not in a default colour of the legend.
#[test]
fn a_bar_series_is_keyed_by_a_swatch_filled_with_its_face_and_outlined_by_its_edge() {
    let (scene, panel) = keyed(|fx, ax| {
        fx.bar(ax, &[0.0, 1.0], &[1.0, 2.0], |b, _| {
            b.label = text("Automatic");
        });
        fx.bar(ax, &[0.0, 1.0], &[1.0, 2.0], |b, _| {
            b.label = text("Red on green");
            b.face = ColorSpec::Rgba { color: RED };
            b.edge.color = ColorSpec::Rgba { color: GREEN };
        });
    });
    let automatic = sample_beside(&scene, panel, "Automatic");
    assert!(
        fills(&automatic).iter().any(|&c| is(c, COLOUR_ORDER[0])),
        "the swatch takes the series colour: {automatic:?}"
    );
    assert!(
        strokes(&automatic).iter().any(|&c| is(c, [0, 0, 0])),
        "and the default black outline: {automatic:?}"
    );
    let fixed = sample_beside(&scene, panel, "Red on green");
    assert!(
        fills(&fixed).iter().any(|&c| is(c, rgb8_of(RED))),
        "{fixed:?}"
    );
    assert!(
        strokes(&fixed).iter().any(|&c| is(c, rgb8_of(GREEN))),
        "{fixed:?}"
    );
}

// Why: bars coloured by data have no one colour, so their key shows the scale they are coloured through, swept from
// its low end at the left to its high end at the right, as the key of a colour-mapped surface does.
#[test]
fn bars_coloured_by_data_are_keyed_by_a_sweep_of_their_scale() {
    let (scene, panel) = keyed(|fx, ax| {
        let scale = fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
            colormap: Colormap::Listed {
                colors: vec![RED, BLUE],
            },
            limits: Limits::Manual { min: 0.0, max: 1.0 },
            ..ContinuousColorScale::default()
        }));
        fx.bar(ax, &[0.0, 1.0], &[1.0, 2.0], |b, fx| {
            b.label = text("Mapped");
            b.face = ColorSpec::Colormapped;
            b.color_data = Some(fx.vector(&[0.0, 1.0]));
            b.color_scale = Some(scale);
        });
    });
    let sample = sample_beside(&scene, panel, "Mapped");
    let b = sample
        .iter()
        .filter_map(Leaf::bbox)
        .reduce(crate::probe::union)
        .expect("a sample is drawn");
    let at = |f: f64| {
        let p = Point::new(b.x + f * b.width, b.y + b.height / 2.0);
        rgb8(fill_under(&sample, p).unwrap_or_else(|| panic!("the sample is filled at {f}")))
    };
    let (red, blue) = (rgb8_of(RED), rgb8_of(BLUE));
    assert!(
        distance(at(0.1), red) < distance(at(0.1), blue),
        "red at the left"
    );
    assert!(
        distance(at(0.9), blue) < distance(at(0.9), red),
        "blue at the right"
    );
}

// Why: a key that lists the same series of bars drawn in two subplots once is how a grouped comparison across panels
// is labelled; entries of the same name and look merge into one entry that stands for both (ADR 0018), and entries
// of the same name but different faces are both listed, because the reader could not tell which colour the name
// stands for.
#[test]
fn bar_entries_of_one_name_merge_when_they_look_alike_and_not_otherwise() {
    let build = |second_face: Color| {
        let mut fx = Fx::new();
        fx.fig.layout.rows = 2;
        fx.fig.layout.cols = 2;
        let left = fx.axes2d(0, 0);
        let right = fx.axes2d(0, 1);
        let panel = fx.panel(cell(1, 0, 1, 2));
        fx.legend_in(left, panel);
        fx.legend_in(right, panel);
        let a = fx.bar(left, &[0.0], &[1.0], |b, _| {
            b.label = Some(Text::plain("Model A"));
            b.face = ColorSpec::Rgba { color: RED };
        });
        let b = fx.bar(right, &[0.0], &[1.0], |b, _| {
            b.label = Some(Text::plain("Model A"));
            b.face = ColorSpec::Rgba { color: second_face };
        });
        (compile_figure(&fx.build()), panel, [a, b])
    };

    let (scene, panel, [a, b]) = build(RED);
    let entries = panel_entries(&scene, panel);
    assert_eq!(entries.len(), 1, "one entry for both: {entries:?}");
    let mut targets: Vec<NodeId> = entries[0].targets.iter().map(|&(_, t)| t).collect();
    targets.sort();
    assert_eq!(targets, vec![a, b]);

    let (scene, panel, _) = build(BLUE);
    assert_eq!(
        panel_entries(&scene, panel).len(),
        2,
        "different faces are both listed"
    );
    assert!(
        scene
            .warnings
            .iter()
            .any(|w| matches!(&w.kind, SceneWarningKind::LegendNameClash { name, .. } if name == "Model A")),
        "the clash of names is reported: {:?}",
        scene.warnings
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Stairs
// ---------------------------------------------------------------------------------------------------------------

// Why: open stairs are a line, so they are keyed by a line sample in the colour of their outline, with nothing
// filled, as a series without markers is; a filled patch would suggest an area the stairs do not draw.
#[test]
fn open_stairs_are_keyed_by_a_line_in_the_colour_of_their_outline() {
    let (scene, panel) = keyed(|fx, ax| {
        fx.stairs(ax, &[0.0, 1.0, 2.0], &[1.0, 2.0], |s| {
            s.label = text("Outline")
        });
    });
    let sample = sample_beside(&scene, panel, "Outline");
    assert!(
        strokes(&sample).iter().any(|&c| is(c, COLOUR_ORDER[0])),
        "a line in the series colour: {sample:?}"
    );
    assert!(fills(&sample).is_empty(), "nothing is filled: {sample:?}");
}

// Why: filled stairs are an area with an outline, as a filled histogram is, so they are keyed by a patch filled in
// their fill colour and outlined in their line colour; keyed by a line alone, they could not be told from open
// stairs of the same colour.
#[test]
fn filled_stairs_are_keyed_by_a_patch_in_their_fill_outlined_by_their_line() {
    let (scene, panel) = keyed(|fx, ax| {
        fx.stairs(ax, &[0.0, 1.0, 2.0], &[1.0, 2.0], |s| {
            s.label = text("Filled");
            s.base = Some(0.0);
            s.fill = ColorSpec::Rgba { color: BLUE };
            s.line.color = ColorSpec::Rgba { color: RED };
        });
    });
    let sample = sample_beside(&scene, panel, "Filled");
    assert!(
        fills(&sample).iter().any(|&c| is(c, rgb8_of(BLUE))),
        "filled: {sample:?}"
    );
    assert!(
        strokes(&sample).iter().any(|&c| is(c, rgb8_of(RED))),
        "outlined: {sample:?}"
    );
}
