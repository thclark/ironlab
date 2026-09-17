//! Tests that the gallery covers every chart type and feature that the MVP must demonstrate.
//!
//! WHY: the gallery is the fixture set of the PDF, viewer and documentation tests. If an entry silently stopped
//! producing, say, a mesh-style surface or linked axes, those tests would keep passing while no longer exercising
//! that feature. The checks inspect the figure IR rather than the source text, so they verify what the facade
//! actually built.

use ironlab::ir::{
    Artist, Axes, ColorSpec, ContourPlacement, Dimension, Figure, Interpreter, MarkerShape, NodeId,
    Projection, Scale,
};
use ironlab_gallery::{all, find};

type Predicate = fn(&Figure, &Axes, &Artist) -> bool;

fn is_2d(axes: &Axes) -> bool {
    matches!(axes.projection, Projection::TwoD)
}

fn is_3d(axes: &Axes) -> bool {
    matches!(axes.projection, Projection::ThreeD { .. })
}

fn figures() -> Vec<(&'static str, Figure)> {
    all()
        .into_iter()
        .map(|entry| (entry.slug, (entry.build)().into_ir()))
        .collect()
}

/// WHY: each row names a MATLAB chart type from the plan and the IR shape that implements it.
#[test]
fn every_required_chart_type_is_present() {
    let requirements: [(&str, Predicate); 15] = [
        ("plot with lines and markers", |_, axes, artist| {
            is_2d(axes)
                && matches!(artist, Artist::Line(l) if l.z.is_none() && l.marker.shape != MarkerShape::None)
        }),
        ("loglog", |_, axes, artist| {
            matches!(artist, Artist::Line(_))
                && axes.x.scale == Scale::Log
                && axes.y.scale == Scale::Log
        }),
        ("semilogx", |_, axes, artist| {
            matches!(artist, Artist::Line(_))
                && axes.x.scale == Scale::Log
                && axes.y.scale == Scale::Linear
        }),
        ("semilogy", |_, axes, artist| {
            matches!(artist, Artist::Line(_))
                && axes.x.scale == Scale::Linear
                && axes.y.scale == Scale::Log
        }),
        ("scatter", |_, axes, artist| {
            is_2d(axes) && matches!(artist, Artist::Scatter(s) if s.z.is_none())
        }),
        ("scatter3", |_, axes, artist| {
            is_3d(axes) && matches!(artist, Artist::Scatter(s) if s.z.is_some())
        }),
        ("contour", |_, axes, artist| {
            is_2d(axes)
                && matches!(artist, Artist::Contour(c) if !c.fill && matches!(c.placement, ContourPlacement::Plane { .. }))
        }),
        ("contourf", |_, axes, artist| {
            is_2d(axes) && matches!(artist, Artist::Contour(c) if c.fill)
        }),
        ("contour3", |_, axes, artist| {
            is_3d(axes)
                && matches!(artist, Artist::Contour(c) if c.placement == ContourPlacement::AtLevel)
        }),
        ("quiver", |_, axes, artist| {
            is_2d(axes) && matches!(artist, Artist::Quiver(q) if q.z.is_none() && q.w.is_none())
        }),
        ("quiver3", |_, axes, artist| {
            is_3d(axes) && matches!(artist, Artist::Quiver(q) if q.z.is_some() && q.w.is_some())
        }),
        ("surf (colormapped faces)", |_, axes, artist| {
            is_3d(axes)
                && matches!(artist, Artist::Surface(s) if s.face == ColorSpec::Colormapped && s.edge != ColorSpec::Colormapped)
        }),
        ("mesh (colormapped edges)", |_, axes, artist| {
            is_3d(axes)
                && matches!(artist, Artist::Surface(s) if s.edge == ColorSpec::Colormapped && s.face != ColorSpec::Colormapped)
        }),
        ("legend with a named series", |_, axes, artist| {
            axes.legend.is_some() && artist.display_name().is_some()
        }),
        ("LaTeX math in an axis label", |_, axes, _| {
            [&axes.x, &axes.y, &axes.z].iter().any(|axis| {
                axis.label
                    .as_ref()
                    .is_some_and(|l| l.interpreter == Interpreter::Latex && l.content.contains('$'))
            })
        }),
    ];

    let figures = figures();
    let missing: Vec<&str> = requirements
        .iter()
        .filter(|(_, predicate)| {
            !figures.iter().any(|(_, figure)| {
                figure.axes.iter().any(|axes| {
                    axes.artists
                        .iter()
                        .any(|artist| predicate(figure, axes, artist))
                })
            })
        })
        .map(|(name, _)| *name)
        .collect();
    assert!(
        missing.is_empty(),
        "no gallery figure demonstrates: {missing:?}"
    );
}

/// WHY: axes titles and figure titles are laid out differently, and both must be exercised.
#[test]
fn axes_titles_are_present() {
    assert!(
        figures()
            .iter()
            .any(|(_, f)| f.axes.iter().any(|a| a.title.is_some())),
        "no gallery figure has an axes title"
    );
}

fn tile(figure: &Figure, row: u32, col: u32) -> NodeId {
    figure
        .axes
        .iter()
        .find(|a| a.cell.row == row && a.cell.col == col)
        .unwrap_or_else(|| panic!("no axes at tile ({row}, {col})"))
        .id
}

fn group(figure: &Figure, id: NodeId, dim: Dimension) -> Vec<NodeId> {
    let mut ids = figure.linked_axes(id, dim);
    ids.sort();
    ids
}

fn sorted(mut ids: Vec<NodeId>) -> Vec<NodeId> {
    ids.sort();
    ids
}

/// WHY: the linked subplots entry is the fixture for link propagation in the viewer, so it must contain a row
/// linked in y, a column linked in x and an arbitrary pair linked in x, with one axes left unlinked as a control.
#[test]
fn linked_subplots_link_a_row_a_column_and_an_arbitrary_pair() {
    let figure = (find("subplots_linked").expect("the entry exists").build)().into_ir();
    assert_eq!((figure.layout.rows, figure.layout.cols), (2, 3));
    let t = |row, col| tile(&figure, row, col);

    assert_eq!(
        group(&figure, t(0, 0), Dimension::Y),
        sorted(vec![t(0, 0), t(0, 1), t(0, 2)])
    );
    assert_eq!(
        group(&figure, t(1, 0), Dimension::Y),
        vec![t(1, 0)],
        "the bottom row is not linked in y"
    );
    assert_eq!(
        group(&figure, t(0, 0), Dimension::X),
        sorted(vec![t(0, 0), t(1, 0)])
    );
    assert_eq!(
        group(&figure, t(0, 1), Dimension::X),
        sorted(vec![t(0, 1), t(1, 2)])
    );
    assert_eq!(
        group(&figure, t(1, 1), Dimension::X),
        vec![t(1, 1)],
        "the control axes is unlinked in x"
    );
    assert_eq!(
        group(&figure, t(1, 1), Dimension::Y),
        vec![t(1, 1)],
        "the control axes is unlinked in y"
    );
}

/// WHY: the all-x and all-y shortcuts must link every axes along one dimension and leave the other dimension free.
#[test]
fn shortcut_entries_link_every_axes_along_one_dimension() {
    for (slug, linked, free) in [
        ("subplots_all_x", Dimension::X, Dimension::Y),
        ("subplots_all_y", Dimension::Y, Dimension::X),
    ] {
        let figure = (find(slug).expect("the entry exists").build)().into_ir();
        assert!(
            figure.axes.len() >= 2,
            "{slug} needs several axes to demonstrate linking"
        );
        let every: Vec<NodeId> = sorted(figure.axes.iter().map(|a| a.id).collect());
        for axes in &figure.axes {
            assert_eq!(
                group(&figure, axes.id, linked),
                every,
                "{slug}: every axes is linked along {linked:?}"
            );
            assert_eq!(
                group(&figure, axes.id, free),
                vec![axes.id],
                "{slug}: no axes is linked along {free:?}"
            );
        }
    }
}

/// WHY: the unlinked subplots entry is the control case for link propagation, so it must have no links at all.
#[test]
fn unlinked_subplots_have_no_links() {
    let figure = (find("subplots_unlinked").expect("the entry exists").build)().into_ir();
    assert_eq!((figure.layout.rows, figure.layout.cols), (2, 2));
    assert_eq!(figure.axes.len(), 4);
    assert!(
        figure.links.is_empty(),
        "unexpected links: {:?}",
        figure.links
    );
}

/// WHY: the legend toggle entry is the fixture for click-to-toggle visibility, which needs four named series with
/// LaTeX names that all start visible.
#[test]
fn legend_toggle_has_four_visible_latex_named_series() {
    let figure = (find("legend_toggle").expect("the entry exists").build)().into_ir();
    let named: Vec<&Artist> = figure
        .axes
        .iter()
        .filter(|a| a.legend.is_some())
        .flat_map(|a| a.artists.iter())
        .filter(|artist| artist.display_name().is_some())
        .collect();
    assert_eq!(named.len(), 4);
    for artist in named {
        let name = artist.display_name().expect("filtered on names");
        assert_eq!(name.interpreter, Interpreter::Latex);
        assert!(
            name.content.contains('$'),
            "the name {:?} contains no math",
            name.content
        );
        assert!(artist.visible(), "every series starts visible");
    }
}

/// WHY: the log axes entry demonstrates the three logarithmic variants side by side, as the plan requires.
#[test]
fn log_axes_has_one_tile_per_variant() {
    let figure = (find("log_axes").expect("the entry exists").build)().into_ir();
    assert_eq!((figure.layout.rows, figure.layout.cols), (1, 3));
    let scales: Vec<(Scale, Scale)> = (0..3)
        .map(|col| {
            let axes = figure
                .axes
                .iter()
                .find(|a| a.cell.col == col)
                .expect("one axes per tile");
            (axes.x.scale, axes.y.scale)
        })
        .collect();
    assert_eq!(
        scales,
        vec![
            (Scale::Log, Scale::Log),
            (Scale::Log, Scale::Linear),
            (Scale::Linear, Scale::Log)
        ]
    );
}
