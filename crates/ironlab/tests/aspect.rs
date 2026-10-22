//! The aspect, the title alignment and the dash patterns of the axes handle, and the warning the export report
//! carries for text beyond the page.

mod common;

use std::path::PathBuf;

use common::{axes, has_error_at};
use ironlab::ir::IssueKind;
use ironlab::prelude::*;
use ironlab::{Dash, ExportReport, SceneWarningKind};

fn temp_path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ironlab-facade-aspect");
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    dir.join(name)
}

// WHY: the aspect, its adjustment and the title alignment are set in user code beside the title and the limits; each
// must reach its own field of the axes, and a figure that uses them correctly must be valid.
#[test]
fn aspect_adjustment_and_title_alignment_set_their_own_fields() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .aspect(Aspect::ratio(2.0))
        .aspect_adjust(AspectAdjust::Limits)
        .title("maximum camber m (p = 0.4, t = 0.12)")
        .title_align(TitleAlign::Left)
        .id();
    let a = axes(&fig, id);
    assert_eq!(a.aspect, Aspect::ratio(2.0));
    assert_eq!(a.aspect_adjust, AspectAdjust::Limits);
    assert_eq!(a.title_align, TitleAlign::Left);
    assert!(fig.validate().is_valid());
}

// WHY: the defaults are the behaviour every figure had before the properties existed, so a figure that does not ask
// for them must not change: an automatic aspect, kept by the box should one be set, and a centred title.
#[test]
fn a_new_axes_has_an_automatic_aspect_and_a_centred_title() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).id();
    let a = axes(&fig, id);
    assert_eq!(a.aspect, Aspect::Auto);
    assert_eq!(a.aspect_adjust, AspectAdjust::Box);
    assert_eq!(a.title_align, TitleAlign::Center);
}

// WHY: a ratio that is not a finite positive number cannot be drawn, and the builder must not panic on it; the user
// learns of it from validation before export, as for invalid limits. The ratio of z is checked on a 2D axes too,
// which ignores it, so that the figure does not become invalid the moment the axes is made three-dimensional.
#[test]
fn an_unusable_ratio_of_y_or_of_z_is_reported_by_validate() {
    for bad in [0.0, -1.5, f64::NAN, f64::INFINITY] {
        for aspect in [
            Aspect::ratio(bad),
            Aspect::Ratio { y: 1.0, z: bad },
            Aspect::Ratio { y: bad, z: 1.0 },
        ] {
            for three_d in [false, true] {
                let mut fig = Figure::new();
                let mut ax = fig.axes(0, 0);
                ax.aspect(aspect);
                if three_d {
                    ax.view(30.0, 20.0);
                }
                let id = ax.id();
                assert!(
                    has_error_at(&fig.validate(), IssueKind::InvalidAspect, id),
                    "{aspect:?}, 3D: {three_d}"
                );
            }
        }
    }
}

// WHY: an isosurface or a mesh seen in 3D is distorted unless one data unit has the same length along x, y and z
// (MATLAB's `daspect([1 1 1])`), so a fixed aspect is valid on a 3D axes, and so is a vertical exaggeration.
#[test]
fn a_fixed_aspect_on_a_3d_axes_is_valid() {
    for aspect in [Aspect::Equal, Aspect::Ratio { y: 1.0, z: 2.0 }] {
        for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
            let mut fig = Figure::new();
            fig.axes(0, 0)
                .aspect(aspect)
                .aspect_adjust(adjust)
                .view(30.0, 20.0);
            assert!(fig.validate().is_valid(), "{aspect:?}, {adjust:?}");
        }
    }
}

// WHY: `Aspect::ratio(y)` is the 2D shorthand; it must mean a ratio of y with z left at one, so that the same figure
// made three-dimensional is not exaggerated along z by accident.
#[test]
fn the_2d_shorthand_leaves_z_at_one() {
    assert_eq!(Aspect::ratio(2.5), Aspect::Ratio { y: 2.5, z: 1.0 });
    assert_eq!(Aspect::Equal.ratios(), Some([1.0, 1.0]));
}

// WHY: five design points overlaid on one plot need five distinct strokes; the two new patterns must be accepted by
// the line handle like the others.
#[test]
fn the_new_dash_patterns_are_set_through_the_line_handle() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let a = ax.plot([0.0, 1.0], [0.0, 1.0]).dash(Dash::DashDotDot).id();
    let b = ax.plot([0.0, 1.0], [1.0, 0.0]).dash(Dash::LongDash).id();
    assert_eq!(common::series(&fig, a).line.dash, Dash::DashDotDot);
    assert_eq!(common::series(&fig, b).line.dash, Dash::LongDash);
    assert!(fig.validate().is_valid());
}

// WHY: a title clipped by the page edge was silent, so a program exporting a figure learnt nothing; the export
// report, which the program reads, must carry the warning, naming the axes and the text, and the page must still be
// written.
#[test]
fn the_export_report_names_a_title_that_extends_beyond_the_page() {
    let title = "Position of maximum camber p, with maximum camber m = 0.04 and thickness t = 0.12 held fixed";
    let mut fig = Figure::new().size_mm(60.0, 50.0);
    let id = fig.axes(0, 0).title(Text::plain(title)).id();
    fig.axes(0, 0).plot([0.0, 1.0], [0.0, 1.0]);
    let path = temp_path("off_page_title.pdf");
    let report: ExportReport = fig.export_pdf(&path).unwrap();
    assert!(path.exists());
    let off_page: Vec<_> = report
        .scene
        .iter()
        .filter(|w| matches!(&w.kind, SceneWarningKind::TextOutsidePage { text } if text == title))
        .collect();
    assert_eq!(off_page.len(), 1, "{:?}", report.scene);
    assert_eq!(off_page[0].node, Some(id));
}
