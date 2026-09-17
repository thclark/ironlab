//! Warnings, degenerate figures and determinism.

use ironlab_ir::{Legend, Levels};

use crate::common::{Fx, compile_figure, linspace, text};
use crate::probe::{axis_maps, from_source, leaves};

// Why: compilation never fails; an artist whose arrays disagree in length is skipped with a warning
// naming it, and the rest of the figure is still drawn.
#[test]
fn invalid_artist_is_skipped_with_a_warning_and_others_still_draw() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let bad = fx.line(ax, &[0.0, 1.0, 2.0], &[0.0, 1.0, 2.0, 3.0], None, |_| {});
    let good = fx.line(ax, &[0.0, 1.0], &[1.0, 0.0], None, |_| {});
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    assert!(
        from_source(&leaves, bad).is_empty(),
        "the invalid line is not drawn"
    );
    assert!(
        !from_source(&leaves, good).is_empty(),
        "the valid line is drawn"
    );
    assert!(
        scene.warnings.iter().any(|w| w.node == Some(bad)),
        "a warning names the invalid line: {:?}",
        scene.warnings
    );
}

// Why: a label with unsupported LaTeX must not stop the figure from building; the text engine's
// warning must surface in the scene, attributed to the node that owns the label.
#[test]
fn unsupported_latex_in_a_label_is_reported_as_a_scene_warning() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
    fx.ax(ax).x.label = text("Distance $\\frobnicate{x}$");
    let scene = compile_figure(&fx.build());
    assert!(
        scene.warnings.iter().any(|w| w.node == Some(ax)),
        "a warning names the axes whose label failed: {:?}",
        scene.warnings
    );
}

// Why: the viewer's problems indicator must point the user at the element whose text failed, so a
// figure title's warning names the figure and a display name's warning names its artist.
#[test]
fn text_warnings_name_the_figure_or_artist_that_owns_the_text() {
    let mut fx = Fx::new();
    fx.fig.title = text("Summary $\\frobnicate{x}$");
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |l| {
        l.display_name = text("Series $\\frobnicate{y}$")
    });
    fx.ax(ax).legend = Some(Legend::default());
    let figure_id = fx.fig.id;
    let scene = compile_figure(&fx.build());
    for (owner, id) in [("figure", figure_id), ("line", line)] {
        assert!(
            scene.warnings.iter().any(|w| w.node == Some(id)),
            "a warning names the {owner}: {:?}",
            scene.warnings
        );
    }
}

// Why: a freshly created axes with no data must still compile to a usable, interactive axes with
// MATLAB's default limits of [0, 1].
#[test]
fn empty_axes_compile_with_unit_limits() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scene = compile_figure(&fx.build());
    let (x, y) = axis_maps(&scene, ax);
    assert_eq!((x.min, x.max), (0.0, 1.0));
    assert_eq!((y.min, y.max), (0.0, 1.0));
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
}

// Why: the viewer recompiles on every change and the PDF must match the screen, so compiling the same
// figure twice must give identical scenes (no hash-map ordering or other hidden state).
#[test]
fn compilation_is_deterministic() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    fx.fig.title = text("Determinism");
    let left = fx.axes2d(0, 0);
    let x = linspace(0.0, 1.0, 20);
    fx.line(left, &x, &x, None, |l| l.display_name = text("$y = x$"));
    fx.scatter(left, &x, &x, None, |s, _| s.display_name = text("Points"));
    fx.ax(left).legend = Some(Legend::default());
    let right = fx.axes2d(0, 1);
    let g = linspace(-1.0, 1.0, 12);
    fx.contour(
        right,
        &g,
        &g,
        |a, b| a * a - b * b,
        |c| {
            c.fill = true;
            c.levels = Levels::Auto { count: 6 };
        },
    );
    let figure = fx.build();
    assert_eq!(compile_figure(&figure), compile_figure(&figure));
}
