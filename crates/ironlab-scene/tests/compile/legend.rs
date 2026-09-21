//! Legends: entry selection and order, placement, LaTeX names and hidden artists.

use ironlab_ir::{Artist, Legend, LegendLocation, Limits, NodeId};
use ironlab_scene::Scene;
use ironlab_scene::display::Rect;
use ironlab_scene::hit::LegendHit;
use ironlab_text::FontId;

use crate::common::{COLOUR_ORDER, Fx, compile_figure, linspace, rgb8, rgb8_close, text, xy};
use crate::probe::{Leaf, axes_hit, from_source, inside, leaves};

/// An axes with a NorthEast legend over three lines, of which the second has no display name.
fn legend_axes() -> (Fx, NodeId, [NodeId; 3]) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let first = fx.line(ax, &[0.0, 1.0], &[0.0, 0.2], None, |l| {
        l.display_name = text("Alpha")
    });
    let unnamed = fx.line(ax, &[0.0, 1.0], &[0.1, 0.3], None, |_| {});
    let third = fx.line(ax, &[0.0, 1.0], &[0.2, 0.4], None, |l| {
        l.display_name = text("$\\beta^2$")
    });
    let axes = fx.ax(ax);
    axes.y.limits = Limits::Manual { min: 0.0, max: 2.0 };
    axes.legend = Some(Legend {
        location: LegendLocation::NorthEast,
        boxed: true,
    });
    (fx, ax, [first, unnamed, third])
}

fn entries(scene: &Scene, ax: NodeId) -> Vec<LegendHit> {
    scene
        .hit_map
        .legend_entries
        .iter()
        .filter(|e| e.axes == ax)
        .cloned()
        .collect()
}

/// Returns the leaves drawn wholly inside a legend entry rectangle.
fn leaves_in(leaves: &[Leaf], rect: Rect) -> Vec<Leaf> {
    leaves
        .iter()
        .filter(|l| l.bbox().is_some_and(|b| inside(b, rect, 0.5)))
        .cloned()
        .collect()
}

// Why: only artists with a display name belong in the legend, listed in drawing order, because the
// legend mirrors MATLAB's `DisplayName` behaviour.
#[test]
fn legend_lists_named_artists_in_artist_order() {
    let (fx, ax, [first, _, third]) = legend_axes();
    let scene = compile_figure(&fx.build());
    let artists: Vec<NodeId> = entries(&scene, ax).iter().map(|e| e.artist).collect();
    assert_eq!(artists, vec![first, third]);
}

// Why: a NorthEast legend sits inside the plot area, stacking entries downwards, so the viewer can map
// clicks on entries back to artists.
#[test]
fn northeast_legend_entries_lie_inside_plot_and_stack_downwards() {
    let (fx, ax, _) = legend_axes();
    let scene = compile_figure(&fx.build());
    let plot = axes_hit(&scene, ax).plot_rect;
    let e = entries(&scene, ax);
    assert_eq!(e.len(), 2);
    for entry in &e {
        assert!(
            inside(entry.rect, plot, 1e-6),
            "{:?} in {plot:?}",
            entry.rect
        );
        assert!(entry.rect.width > 0.0 && entry.rect.height > 0.0);
    }
    assert!(
        e[0].rect.bottom() <= e[1].rect.y + 1e-6,
        "first entry above second"
    );
    let centre_x = plot.x + plot.width / 2.0;
    let centre_y = plot.y + plot.height / 2.0;
    assert!(
        e[0].rect.x > centre_x && e[0].rect.y < centre_y,
        "entries in the top-right quadrant"
    );
}

// Why: a legend entry is only useful if its sample looks like the series it names, so the sample of
// an automatically coloured line takes that line's colour; its hit rectangle must enclose both the
// sample and the label, so clicking either toggles the artist; and the whole legend belongs to the
// axes rather than to the artist, so hiding the artist cannot remove it.
#[test]
fn entry_holds_a_sample_in_the_artist_colour_and_a_label_attributed_to_the_axes() {
    let (fx, ax, [_, _, third]) = legend_axes();
    let scene = compile_figure(&fx.build());
    let entry = entries(&scene, ax)
        .into_iter()
        .find(|e| e.artist == third)
        .expect("entry for the third line");
    let leaves = leaves(&scene);
    let in_entry = leaves_in(&leaves, entry.rect);
    assert!(
        in_entry.iter().any(|l| l.glyphs().is_some()),
        "the label lies inside the entry"
    );
    let samples: Vec<[u8; 3]> = in_entry
        .iter()
        .filter_map(|l| l.path().and_then(|p| p.stroke.as_ref()))
        .map(|s| rgb8(s.color))
        .collect();
    assert!(
        !samples.is_empty(),
        "a stroked sample lies inside the entry"
    );
    let line_colour = COLOUR_ORDER[2];
    assert!(
        samples.iter().all(|c| rgb8_close(*c, line_colour)),
        "the sample of the third automatic line is {line_colour:?}: {samples:?}"
    );
    assert!(
        in_entry.iter().all(|l| l.source == Some(ax)),
        "legend items name the axes as their source"
    );
}

// Why: the legend overlays the data, so it must be painted after every artist of its axes, or a
// series could be drawn across the legend and hide it.
#[test]
fn legend_is_painted_after_every_artist() {
    let (fx, ax, artists) = legend_axes();
    let scene = compile_figure(&fx.build());
    let e = entries(&scene, ax);
    let leaves = leaves(&scene);
    let last_artist_item = (0..leaves.len())
        .filter(|i| artists.iter().any(|a| leaves[*i].source == Some(*a)))
        .max()
        .expect("artists are drawn");
    for entry in &e {
        let first_entry_item = (0..leaves.len())
            .filter(|i| {
                leaves[*i].source == Some(ax)
                    && leaves[*i]
                        .bbox()
                        .is_some_and(|b| inside(b, entry.rect, 0.5))
            })
            .min()
            .expect("entry items are drawn");
        assert!(
            first_entry_item > last_artist_item,
            "entry item {first_entry_item} is painted after artist item {last_artist_item}"
        );
    }
}

// Why: display names are LaTeX-interpreted text, so math in a legend label must be typeset in the
// math font inside its entry.
#[test]
fn latex_display_name_is_typeset_as_math_in_its_entry() {
    let (fx, ax, [_, _, third]) = legend_axes();
    let scene = compile_figure(&fx.build());
    let entry = entries(&scene, ax)
        .into_iter()
        .find(|e| e.artist == third)
        .expect("entry for the LaTeX-named line");
    let leaves = leaves(&scene);
    let inside_entry = leaves_in(&leaves, entry.rect);
    assert!(
        inside_entry
            .iter()
            .any(|l| l.glyphs().is_some_and(|g| g.font == FontId::Math)),
        "math glyph runs are drawn inside the entry"
    );
}

// Why: hiding an artist (a legend click in the viewer) removes it from the plot but keeps its entry,
// greyed out, so the user can click it again to restore the artist.
#[test]
fn hidden_artist_is_not_drawn_but_keeps_a_greyed_legend_entry() {
    let (mut fx, ax, [first, _, third]) = legend_axes();
    if let Some(ironlab_ir::Artist::Line(l)) = fx.ax(ax).artists.first_mut() {
        l.visible = false;
    }
    let scene = compile_figure(&fx.build());
    let e = entries(&scene, ax);
    let artists: Vec<NodeId> = e.iter().map(|e| e.artist).collect();
    assert_eq!(
        artists,
        vec![first, third],
        "the hidden artist keeps its entry"
    );
    let leaves = leaves(&scene);

    assert!(
        from_source(&leaves, first).is_empty(),
        "the hidden line draws nothing (legend entries name the axes as their source)"
    );

    let hidden_rect = e[0].rect;
    let greyed = leaves_in(&leaves, hidden_rect);
    assert!(!greyed.is_empty(), "the hidden artist's entry is drawn");
    for leaf in &greyed {
        for c in leaf.colors() {
            assert!(c.a < 1.0, "hidden entry item is greyed: {c:?}");
        }
    }

    let normal = leaves_in(&leaves, e[1].rect);
    assert!(
        normal.iter().flat_map(|l| l.colors()).any(|c| c.a == 1.0),
        "a visible artist's entry is drawn at full opacity"
    );
}

// WHY: the legend names every kind of artist that has a display name, in artist order, and gives each a sample and
// a label, so that a legend over a figure mixing lines with images still matches entries to artists; a kind that
// dropped out of the legend, or whose sample was left blank, would leave its name unexplained.
#[test]
fn legend_lists_named_artists_of_every_kind_in_artist_order() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let g = linspace(0.0, 1.0, 4);
    let mut ids = vec![
        fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |l| {
            l.display_name = text("Line")
        }),
        fx.scatter(ax, &[0.2, 0.8], &[0.5, 0.5], None, |s, _| {
            s.display_name = text("Scatter")
        }),
        fx.contour(
            ax,
            &g,
            &g,
            |x, y| x + y,
            |c| c.display_name = text("Contour"),
        ),
    ];
    let quiver = fx.quiver(ax, &[0.5], &[0.5], None, &[0.1], &[0.1], None);
    if let Some(Artist::Quiver(q)) = fx.ax(ax).artists.last_mut() {
        q.display_name = text("Quiver");
    }
    ids.push(quiver);
    ids.extend([
        fx.surface(
            ax,
            &g,
            &g,
            |x, y| x - y,
            |s| s.display_name = text("Surface"),
        ),
        fx.image(
            ax,
            vec![1, 2, 3],
            vec![255u8, 0, 0, 0, 0, 255],
            xy(None, None),
            |i| i.display_name = text("Image"),
        ),
        fx.indexed_image(ax, vec![1, 2], vec![0u8, 255], xy(None, None), |i| {
            i.display_name = text("Indexed")
        }),
        fx.mapped_image(ax, vec![1, 2], vec![0.0, 1.0], xy(None, None), |m| {
            m.display_name = text("Mapped")
        }),
    ]);
    fx.ax(ax).legend = Some(Legend::default());
    let scene = compile_figure(&fx.build());
    let e = entries(&scene, ax);
    assert_eq!(
        e.iter().map(|e| e.artist).collect::<Vec<_>>(),
        ids,
        "one entry per named artist of every kind, in artist order"
    );
    let leaves = leaves(&scene);
    for entry in &e {
        let in_entry = leaves_in(&leaves, entry.rect);
        assert!(
            in_entry.iter().any(|l| l.glyphs().is_some()),
            "the entry of {} holds a label",
            entry.artist
        );
        assert!(
            in_entry.iter().any(|l| l.path().is_some()),
            "the entry of {} holds a sample",
            entry.artist
        );
        assert!(
            in_entry.iter().all(|l| l.source == Some(ax)),
            "the entry of {} names the axes as its source",
            entry.artist
        );
    }
}
