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
        .aspect(Aspect::Ratio(2.0))
        .aspect_adjust(AspectAdjust::Limits)
        .title("maximum camber m (p = 0.4, t = 0.12)")
        .title_align(TitleAlign::Left)
        .id();
    let a = axes(&fig, id);
    assert_eq!(a.aspect, Aspect::Ratio(2.0));
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
// learns of it from validation before export, as for invalid limits.
#[test]
fn an_unusable_ratio_is_reported_by_validate() {
    for ratio in [0.0, -1.5, f64::NAN, f64::INFINITY] {
        let mut fig = Figure::new();
        let id = fig.axes(0, 0).aspect(Aspect::Ratio(ratio)).id();
        assert!(
            has_error_at(&fig.validate(), IssueKind::InvalidAspect, id),
            "ratio {ratio}"
        );
    }
}

// WHY: the aspect shapes the plot area of a 2D axes; on a 3D axes it would be silently ignored, so validation names
// the mistake instead.
#[test]
fn a_fixed_aspect_on_a_3d_axes_is_reported_by_validate() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).aspect(Aspect::Equal).view(30.0, 20.0).id();
    assert!(has_error_at(&fig.validate(), IssueKind::InvalidAspect, id));
}

// WHY: five design points overlaid on one plot need five distinct strokes; the two new patterns must be accepted by
// the line handle like the others.
#[test]
fn the_new_dash_patterns_are_set_through_the_line_handle() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let a = ax.plot([0.0, 1.0], [0.0, 1.0]).dash(Dash::DashDotDot).id();
    let b = ax.plot([0.0, 1.0], [1.0, 0.0]).dash(Dash::LongDash).id();
    assert_eq!(common::line(&fig, a).line.dash, Dash::DashDotDot);
    assert_eq!(common::line(&fig, b).line.dash, Dash::LongDash);
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
