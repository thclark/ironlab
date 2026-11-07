//! Emboldening: a figure drawn heavier for projection without changing what it shows.
//!
//! Why: a figure that reads well on paper has lines and text too fine for a projector, and an author must be able to
//! draw the same figure heavier without rebuilding it. Every line and edge must thicken by the step's line scale and
//! every label must be drawn heavier, while marker sizes (which can encode data), tick lengths and images stay as they
//! are. A single stroke compiled from a width that bypasses the scale would leave one line thin on the slide, so these
//! tests compile one figure holding every kind of stroked thing and compare its widths source by source.
//!
//! Heavier text measures wider, so the layout of an emboldened figure may shift slightly; the tests therefore compare
//! the distinct values drawn by each source rather than the items one by one.

use std::collections::{BTreeMap, BTreeSet};

use ironlab_ir::{
    Color, ColorSpec, DashStyle, Embolden, Figure, InsideColorbar, InsideLegend, Legend, LineStyle,
    MarkerShape, MarkerStyle, NodeId, View3d,
};
use ironlab_scene::display::ItemKind;
use ironlab_scene::{CompileOptions, Scene, compile, compile_with};
use ironlab_text::FontId;

use crate::common::{Fx, engine, linspace, text, xy};
use crate::probe::{Leaf, from_source, glyph_runs, leaves};

/// Relative tolerance for comparing a scaled width with the width it was scaled from.
const REL: f64 = 1e-9;

/// The identifiers of the nodes of [`many_kinds`] that must draw strokes.
struct Kinds {
    line_axes: NodeId,
    field_axes: NodeId,
    cells_axes: NodeId,
    surface_axes: NodeId,
    marked_line: NodeId,
    thick_line: NodeId,
    dashed_line: NodeId,
    sized_scatter: NodeId,
    bar: NodeId,
    stairs: NodeId,
    contour: NodeId,
    quiver: NodeId,
    cells: NodeId,
    surface: NodeId,
    image: NodeId,
}

/// The diameters, in points, of the markers of the scatter sized by data.
const SIZE_DATA: [f64; 3] = [3.0, 9.0, 15.0];

/// A figure of four axes that between them draw every kind of stroked line and edge the compiler emits: an axes box
/// with ticks and grid lines, a line with markers, a line too thick for its legend sample and marker edges (which the
/// compiler clamps, so the scale must apply after the clamp), a dashed line, a scatter sized by data, bars, stairs, a
/// legend, a contour, a quiver, an inside colourbar, outlined cells, an image, and a 3D axes with an edged surface. Its
/// labels mix plain text, bold text and mathematics.
fn many_kinds() -> (Figure, Kinds) {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 2;
    fx.fig.title = text("Pressure field");

    let line_axes = fx.axes2d(0, 0);
    let xs = linspace(0.0, 4.0, 9);
    let marked_line = fx.line(
        line_axes,
        &xs,
        &xs.iter().map(|x| x * 0.5).collect::<Vec<_>>(),
        None,
        |s| {
            s.label = text("Lift $C_L$");
            s.line.width_pt = 1.25;
            s.marker = MarkerStyle {
                shape: MarkerShape::Circle,
                size_pt: 5.0,
                ..MarkerStyle::default()
            };
        },
    );
    let thick_line = fx.line(
        line_axes,
        &xs,
        &xs.iter().map(|x| 1.0 + x * 0.1).collect::<Vec<_>>(),
        None,
        |s| {
            s.label = text("\\textbf{Envelope}");
            s.line.width_pt = 3.5;
            s.marker = MarkerStyle {
                shape: MarkerShape::Square,
                size_pt: 6.0,
                ..MarkerStyle::default()
            };
        },
    );
    let dashed_line = fx.line(
        line_axes,
        &xs,
        &xs.iter().map(|x| 2.0 - x * 0.25).collect::<Vec<_>>(),
        None,
        |s| {
            s.label = text("Drag");
            s.line.dash = DashStyle::Dashed;
            s.line.width_pt = 0.75;
        },
    );
    let bar = fx.bar(line_axes, &[0.5, 1.5, 2.5], &[1.0, 0.4, 0.7], |b, _| {
        b.label = text("Count");
        b.edge.width_pt = 0.6;
    });
    let stairs = fx.stairs(line_axes, &[0.0, 1.0, 2.0, 3.0], &[0.2, 0.9, 0.5], |s| {
        s.label = text("$\\bar{u}$");
    });
    {
        let axes = fx.ax(line_axes);
        axes.x.grid = true;
        axes.y.grid = true;
        axes.x.label = text("Time (s)");
        axes.y.label = text("$\\Delta p$ (kPa)");
        axes.legend = Some(Legend::Inside(InsideLegend {
            boxed: true,
            ..InsideLegend::default()
        }));
    }

    let field_axes = fx.axes2d(0, 1);
    let grid = linspace(-1.0, 1.0, 12);
    let contour = fx.contour(field_axes, &grid, &grid, |x, y| x * x + y * y, |_| {});
    let quiver = fx.quiver(
        field_axes,
        &[-0.5, 0.0, 0.5],
        &[0.0, 0.5, -0.5],
        None,
        &[0.2, 0.1, -0.1],
        &[0.1, -0.2, 0.2],
        None,
    );
    let sized_scatter = fx.scatter(
        field_axes,
        &[-0.6, 0.0, 0.6],
        &[0.6, -0.6, 0.0],
        None,
        |s, fx| {
            s.size_data = Some(fx.vector(&SIZE_DATA));
            s.marker.edge = ColorSpec::Rgba {
                color: Color::BLACK,
            };
        },
    );
    let scale = fx.axes_scale(field_axes);
    fx.ax(field_axes).colorbar = Some(InsideColorbar {
        scale,
        ..InsideColorbar::default()
    });

    let cells_axes = fx.axes2d(1, 0);
    let cells_scale = fx.axes_scale(cells_axes);
    let cells = fx.cells(
        cells_axes,
        vec![2, 3],
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        Some(cells_scale),
        |c, _| {
            c.edge = LineStyle {
                color: ColorSpec::Rgba {
                    color: Color::BLACK,
                },
                width_pt: 0.8,
                ..c.edge
            };
        },
    );
    let image = fx.imagesc(
        cells_axes,
        vec![2, 2],
        vec![0.0, 1.0, 2.0, 3.0],
        xy(None, None),
        |_| {},
    );

    let surface_axes = fx.axes3d(1, 1, View3d::default());
    let ring = linspace(-1.0, 1.0, 6);
    let surface = fx.surface(
        surface_axes,
        &ring,
        &ring,
        |x, y| x * y,
        |s| {
            s.edge = ColorSpec::Rgba {
                color: Color::BLACK,
            };
            s.edge_width_pt = 0.4;
        },
    );
    fx.ax(surface_axes).x.grid = true;

    let kinds = Kinds {
        line_axes,
        field_axes,
        cells_axes,
        surface_axes,
        marked_line,
        thick_line,
        dashed_line,
        sized_scatter,
        bar,
        stairs,
        contour,
        quiver,
        cells,
        surface,
        image,
    };
    (fx.build(), kinds)
}

/// Compiles `figure` drawn at `step`, stored in the figure as an author would store it.
fn at(figure: &Figure, step: Embolden) -> Scene {
    let mut figure = figure.clone();
    figure.embolden = step;
    compile(&figure, engine())
}

/// Collects a value for every leaf, grouped by source and sorted and deduplicated within each source.
fn per_source(
    scene: &Scene,
    value: impl Fn(&Leaf) -> Vec<f64>,
) -> BTreeMap<Option<NodeId>, Vec<f64>> {
    let mut out: BTreeMap<Option<NodeId>, Vec<f64>> = BTreeMap::new();
    for leaf in leaves(scene) {
        let values = value(&leaf);
        if !values.is_empty() {
            out.entry(leaf.source).or_default().extend(values);
        }
    }
    for values in out.values_mut() {
        values.sort_by(f64::total_cmp);
        values.dedup_by(|a, b| (*a - *b).abs() <= REL * b.abs().max(1.0));
    }
    out
}

/// The width of every stroked path and every marker edge a leaf draws.
fn stroke_widths(leaf: &Leaf) -> Vec<f64> {
    match &leaf.kind {
        ItemKind::Path(path) => path.stroke.iter().map(|s| s.width).collect(),
        ItemKind::Markers(markers) if markers.instances.iter().any(|m| m.edge.is_some()) => {
            vec![markers.edge_width]
        }
        _ => Vec::new(),
    }
}

/// Asserts that every value of `heavy` is the corresponding value of `regular` multiplied by `scale`, source by
/// source, and that both scenes have values for the same sources.
#[track_caller]
fn assert_scaled(
    regular: &BTreeMap<Option<NodeId>, Vec<f64>>,
    heavy: &BTreeMap<Option<NodeId>, Vec<f64>>,
    scale: f64,
    what: &str,
) {
    assert_eq!(
        regular.keys().collect::<Vec<_>>(),
        heavy.keys().collect::<Vec<_>>(),
        "the same sources draw {what}"
    );
    for (source, values) in regular {
        let expected: Vec<f64> = values.iter().map(|v| v * scale).collect();
        let got = &heavy[source];
        assert_eq!(
            got.len(),
            expected.len(),
            "{what} of {source:?}: {got:?} against {expected:?}"
        );
        for (g, e) in got.iter().zip(&expected) {
            assert!(
                (g - e).abs() <= REL * e.abs().max(1.0),
                "{what} of {source:?}: {got:?} against {values:?} × {scale}"
            );
        }
    }
}

// Why: every line and edge of the figure, from the axes box and its ticks to the edges of cells and surfaces, must
// thicken by the step's line scale, so that the lines keep their proportions to one another and to the text; a source
// whose widths do not scale is a call site that bypasses the scale and leaves its lines thin on the slide.
#[test]
fn every_stroke_width_is_multiplied_by_the_line_scale_of_the_step() {
    let (figure, kinds) = many_kinds();
    let regular = per_source(&at(&figure, Embolden::Regular), stroke_widths);
    for source in [
        kinds.line_axes,
        kinds.field_axes,
        kinds.cells_axes,
        kinds.surface_axes,
        kinds.marked_line,
        kinds.thick_line,
        kinds.dashed_line,
        kinds.sized_scatter,
        kinds.bar,
        kinds.stairs,
        kinds.contour,
        kinds.quiver,
        kinds.cells,
        kinds.surface,
    ] {
        assert!(
            regular.contains_key(&Some(source)),
            "the fixture strokes {source:?}: {regular:?}"
        );
    }
    for step in [Embolden::Medium, Embolden::Semibold] {
        let heavy = per_source(&at(&figure, step), stroke_widths);
        assert_scaled(
            &regular,
            &heavy,
            step_scale(step),
            &format!("stroke widths at {step:?}"),
        );
    }
}

/// The line scale of a step, from the text crate, which calibrates it from the fonts.
fn step_scale(step: Embolden) -> f64 {
    text_step(step).line_scale()
}

/// The step of the text crate that corresponds to a step of the IR.
fn text_step(step: Embolden) -> ironlab_text::Embolden {
    match step {
        Embolden::Regular => ironlab_text::Embolden::Regular,
        Embolden::Medium => ironlab_text::Embolden::Medium,
        Embolden::Semibold => ironlab_text::Embolden::Semibold,
    }
}

// Why: a dash pattern is drawn in lengths derived from the line's width, so that a dashed line keeps its look as it
// thickens; dashes that stayed the same length on a heavier line would close up into a solid one.
#[test]
fn dash_patterns_scale_with_the_line() {
    let (figure, kinds) = many_kinds();
    let dashes = |scene: &Scene| -> Vec<Vec<f64>> {
        let mut out: Vec<Vec<f64>> = from_source(&leaves(scene), kinds.dashed_line)
            .iter()
            .filter_map(|leaf| leaf.path().and_then(|p| p.stroke.as_ref()))
            .map(|stroke| stroke.dash.clone())
            .filter(|dash| !dash.is_empty())
            .collect();
        out.dedup();
        out
    };
    let regular = dashes(&at(&figure, Embolden::Regular));
    assert!(!regular.is_empty(), "the dashed line is drawn dashed");
    let scale = step_scale(Embolden::Semibold);
    let heavy = dashes(&at(&figure, Embolden::Semibold));
    assert_eq!(heavy.len(), regular.len());
    for (h, r) in heavy.iter().zip(&regular) {
        assert_eq!(h.len(), r.len());
        for (a, b) in h.iter().zip(r) {
            assert!(
                (a - b * scale).abs() <= REL * (b * scale).max(1.0),
                "{h:?} against {r:?} × {scale}"
            );
        }
    }
}

// Why: the size of a marker can represent data (a bubble chart), so emboldening must never change it, whereas its edge
// is a line like any other and thickens (which the stroke-width test checks). The scatter sized by data is the case
// that matters: its diameters are values of the figure's data and must reach the page as given.
#[test]
fn marker_sizes_are_unchanged() {
    let (figure, kinds) = many_kinds();
    let sizes = |scene: &Scene| {
        per_source(scene, |leaf| {
            leaf.instances().iter().map(|m| m.size_pt).collect()
        })
    };
    let regular = sizes(&at(&figure, Embolden::Regular));
    for source in [kinds.marked_line, kinds.thick_line] {
        assert!(regular.contains_key(&Some(source)), "{regular:?}");
    }
    assert_eq!(sizes(&at(&figure, Embolden::Semibold)), regular);

    let sized = |scene: &Scene| {
        let mut sizes: Vec<f64> =
            crate::probe::marker_instances(&leaves(scene), kinds.sized_scatter)
                .iter()
                .map(|m| m.size_pt)
                .collect();
        sizes.sort_by(f64::total_cmp);
        sizes
    };
    for step in [Embolden::Regular, Embolden::Medium, Embolden::Semibold] {
        assert_eq!(
            sized(&at(&figure, step)),
            SIZE_DATA.to_vec(),
            "{step:?}: each marker of the sized scatter keeps the diameter of its datum"
        );
    }
}

/// The lengths of the tick marks an axes draws: the straight two-point subpaths of its stroked paths that are shorter
/// than a font size, which grid lines and box edges never are.
fn tick_lengths(scene: &Scene, axes: NodeId) -> Vec<f64> {
    let short = figure_font_size();
    let mut lengths: Vec<f64> = from_source(&leaves(scene), axes)
        .iter()
        .filter(|leaf| {
            leaf.path()
                .is_some_and(|p| p.stroke.is_some() && p.fill.is_none())
        })
        .flat_map(|leaf| leaf.subpaths())
        .filter(|points| points.len() == 2)
        .map(|points| (points[1].x - points[0].x).hypot(points[1].y - points[0].y))
        .filter(|&length| length > 0.0 && length < short)
        .map(|length| (length * 1e6).round() / 1e6)
        .collect();
    lengths.sort_by(f64::total_cmp);
    lengths.dedup();
    lengths
}

/// The base font size of a default figure, in points.
fn figure_font_size() -> f64 {
    Figure::default().font_size_pt
}

// Why: tick marks are measured against the plot, not drawn as lines of a weight, so they must keep their length when
// they thicken; ticks that grew with the step would crowd the tick labels of a slide.
#[test]
fn tick_lengths_are_unchanged() {
    let (figure, kinds) = many_kinds();
    let regular_scene = at(&figure, Embolden::Regular);
    let heavy_scene = at(&figure, Embolden::Semibold);
    for axes in [kinds.line_axes, kinds.field_axes, kinds.surface_axes] {
        let regular = tick_lengths(&regular_scene, axes);
        assert!(!regular.is_empty(), "{axes:?} draws ticks");
        assert_eq!(tick_lengths(&heavy_scene, axes), regular, "{axes:?}");
    }
}

// Why: an image is the user's pixels, so emboldening must not resample or resize it; only the lines around it thicken.
#[test]
fn images_are_unchanged() {
    let (figure, kinds) = many_kinds();
    let images = |scene: &Scene| {
        from_source(&leaves(scene), kinds.image)
            .iter()
            .filter_map(|leaf| leaf.image().cloned())
            .map(|image| {
                (
                    image.rect.width,
                    image.rect.height,
                    image.width,
                    image.height,
                    image.samples,
                )
            })
            .collect::<Vec<_>>()
    };
    let regular = images(&at(&figure, Embolden::Regular));
    assert!(!regular.is_empty(), "the image is drawn");
    assert_eq!(images(&at(&figure, Embolden::Semibold)), regular);
}

// Why: plain labels and tick labels must be set in the designed face of the step, and mathematics and bold text (which
// have no heavier face) must be stroked by the step's stroke, so that a slide's labels are heavier throughout and its maths
// matches the text beside it. At the regular step nothing may be stroked and no heavier face used, so that print
// figures are drawn exactly as designed.
#[test]
fn labels_are_drawn_in_the_face_and_stroke_of_the_step() {
    let (figure, _) = many_kinds();
    for step in [Embolden::Regular, Embolden::Medium, Embolden::Semibold] {
        let scene = at(&figure, step);
        let leaves = leaves(&scene);
        let runs = glyph_runs(&leaves);
        let plain_face = match step {
            Embolden::Regular => FontId::TextRegular,
            Embolden::Medium => FontId::TextMedium,
            Embolden::Semibold => FontId::TextSemibold,
        };
        let stroke = text_step(step).stroke_em();
        let mut plain = 0;
        let mut stroked_kinds = BTreeSet::new();
        for (_, run) in &runs {
            match run.font {
                FontId::Math | FontId::TextBold => {
                    stroked_kinds.insert(format!("{:?}", run.font));
                    assert!(
                        (run.stroke_em - stroke).abs() < 1e-12,
                        "{step:?}: {:?} is stroked {}",
                        run.text,
                        run.stroke_em
                    );
                }
                font => {
                    plain += 1;
                    assert_eq!(font, plain_face, "{step:?}: {:?}", run.text);
                    assert_eq!(
                        run.stroke_em, 0.0,
                        "{step:?}: plain {:?} is not stroked",
                        run.text
                    );
                }
            }
        }
        assert!(
            plain > 0 && stroked_kinds.len() == 2,
            "the fixture draws plain, bold and mathematical text: {stroked_kinds:?}"
        );
    }
}

// Why: an export draws a figure at a step other than its own without changing it, so overriding the step must draw
// exactly what the figure would draw had it been stored at that step, lighter or heavier, and no override must draw
// the figure's own (which is also what `compile`, used by the viewer, draws).
#[test]
fn an_override_draws_the_figure_as_if_it_were_stored_at_that_step() {
    let (figure, _) = many_kinds();
    let text = engine();
    let over = |stored: Embolden, step: Option<Embolden>| {
        let mut figure = figure.clone();
        figure.embolden = stored;
        compile_with(&figure, text, &CompileOptions { embolden: step })
    };
    assert_eq!(
        over(Embolden::Regular, Some(Embolden::Semibold)),
        at(&figure, Embolden::Semibold)
    );
    assert_eq!(
        over(Embolden::Semibold, Some(Embolden::Regular)),
        at(&figure, Embolden::Regular)
    );
    assert_eq!(over(Embolden::Medium, None), at(&figure, Embolden::Medium));
    // The override did draw something different from the stored step.
    assert_ne!(
        over(Embolden::Regular, Some(Embolden::Semibold)),
        over(Embolden::Regular, None)
    );
}

// Why: a scene drawn heavier keeps the figure's structure: the same nodes draw items, with the same kinds, so that
// picking, the legend and the hit map work on a slide figure as they do on the printed one.
#[test]
fn emboldening_draws_the_same_nodes() {
    let (figure, _) = many_kinds();
    let sources = |scene: &Scene| {
        leaves(scene)
            .iter()
            .map(|leaf| leaf.source)
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        sources(&at(&figure, Embolden::Semibold)),
        sources(&at(&figure, Embolden::Regular))
    );
}

// Why: the step must only thicken what is drawn. A figure with no axes draws no line and no text, so it must be drawn
// alike at every step, rather than gaining items or failing for want of anything to scale.
#[test]
fn a_figure_with_nothing_to_thicken_is_drawn_alike_at_every_step() {
    let figure = Figure::default();
    let regular = at(&figure, Embolden::Regular);
    for step in [Embolden::Medium, Embolden::Semibold] {
        assert_eq!(at(&figure, step), regular, "{step:?}");
    }
}
