//! Axes property setters: titles, labels, limits, scales, grid, legend, colormap,
//! colour limits and box.

mod common;

use common::{axes, has_error_at};
use ironlab::ir::{ColormapName, IssueKind, Legend, Limits};
use ironlab::prelude::*;

// WHY: titles and labels are the most common calls in user code; each must reach its
// own axis, and plain strings must keep the LaTeX interpreter so that `$…$` labels are
// typeset.
#[test]
fn title_and_labels_set_their_own_fields() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .title("Response")
        .xlabel("$t$ (s)")
        .ylabel("$u$")
        .zlabel("$w$")
        .id();
    let a = axes(&fig, id);
    assert_eq!(a.title, Some(Text::new("Response")));
    assert_eq!(a.x.label, Some(Text::new("$t$ (s)")));
    assert_eq!(a.y.label, Some(Text::new("$u$")));
    assert_eq!(a.z.label, Some(Text::new("$w$")));
}

// WHY: xlim/ylim/zlim fix the range of one dimension each; a mix-up changes the wrong
// axis without any visible error in code review.
#[test]
fn limit_setters_fix_their_own_dimension() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .xlim(-1.0, 1.0)
        .ylim(0.0, 10.0)
        .zlim(5.0, 6.0)
        .id();
    let a = axes(&fig, id);
    assert_eq!(
        a.x.limits,
        Limits::Manual {
            min: -1.0,
            max: 1.0
        }
    );
    assert_eq!(
        a.y.limits,
        Limits::Manual {
            min: 0.0,
            max: 10.0
        }
    );
    assert_eq!(a.z.limits, Limits::Manual { min: 5.0, max: 6.0 });
}

// WHY: linked axes promise that setting limits on one sets them on its group, in the
// API as well as in the viewer; otherwise a linked gallery figure starts out of sync.
#[test]
fn xlim_propagates_to_linked_axes_only_along_x() {
    let mut fig = Figure::new().tiles(1, 3);
    let a = fig.axes(0, 0).id();
    let b = fig.axes(0, 1).id();
    let c = fig.axes(0, 2).id();
    fig.link(Dim::X, &[a, b]).unwrap();

    fig.axes(0, 0).xlim(2.0, 3.0);

    let manual = Limits::Manual { min: 2.0, max: 3.0 };
    assert_eq!(axes(&fig, a).x.limits, manual);
    assert_eq!(axes(&fig, b).x.limits, manual);
    assert_eq!(axes(&fig, c).x.limits, Limits::Auto);
    assert_eq!(axes(&fig, b).y.limits, Limits::Auto);
}

// WHY: each limit setter must propagate through the link group of its own dimension.
// With x, y and z each linked to a different partner, a setter that consults the wrong
// dimension's group moves the wrong subplot, which no single-dimension test detects.
// Setting the limits on the second member of a group checks that propagation does not
// depend on which member was linked first.
#[test]
fn every_limit_setter_propagates_only_through_its_own_link_group() {
    let mut fig = Figure::new().tiles(2, 2);
    let a = fig.axes(0, 0).id();
    let bx = fig.axes(0, 1).id();
    let cy = fig.axes(1, 0).id();
    let dz = fig.axes(1, 1).id();
    fig.link(Dim::X, &[bx, a]).unwrap();
    fig.link(Dim::Y, &[a, cy]).unwrap();
    fig.link(Dim::Z, &[a, dz]).unwrap();

    fig.axes(0, 0).xlim(1.0, 2.0).ylim(3.0, 4.0).zlim(5.0, 6.0);

    let x = Limits::Manual { min: 1.0, max: 2.0 };
    let y = Limits::Manual { min: 3.0, max: 4.0 };
    let z = Limits::Manual { min: 5.0, max: 6.0 };
    let limits = |id| {
        let axes = axes(&fig, id);
        (axes.x.limits, axes.y.limits, axes.z.limits)
    };
    assert_eq!(limits(a), (x, y, z));
    assert_eq!(limits(bx), (x, Limits::Auto, Limits::Auto));
    assert_eq!(limits(cy), (Limits::Auto, y, Limits::Auto));
    assert_eq!(limits(dz), (Limits::Auto, Limits::Auto, z));
}

// WHY: invalid limits must not panic in the builder (the facade's contract), and must
// be reported by validation so that the user learns about them before export.
#[test]
fn invalid_limits_do_not_panic_and_are_reported_by_validate() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).xlim(1.0, 0.0).id();
    assert!(has_error_at(&fig.validate(), IssueKind::InvalidLimits, id));

    let mut fig = Figure::new();
    let id = fig.axes(0, 0).ylim(f64::NAN, 1.0).id();
    assert!(has_error_at(&fig.validate(), IssueKind::InvalidLimits, id));
}

// WHY: invalid limits are documented to be stored on the axes they were set on only;
// copying them to linked axes would multiply one mistake into several errors and
// overwrite the partners' valid limits.
#[test]
fn invalid_limits_are_not_propagated_to_linked_axes() {
    let mut fig = Figure::new().tiles(1, 2);
    let a = fig.axes(0, 0).id();
    let b = fig.axes(0, 1).id();
    fig.link(Dim::X, &[a, b]).unwrap();
    fig.axes(0, 1).xlim(0.0, 1.0);

    fig.axes(0, 0).xlim(1.0, 0.0);

    assert_eq!(
        axes(&fig, a).x.limits,
        Limits::Manual { min: 1.0, max: 0.0 }
    );
    assert_eq!(
        axes(&fig, b).x.limits,
        Limits::Manual { min: 0.0, max: 1.0 }
    );
}

// WHY: scale setters must change only their own axis.
#[test]
fn scale_setters_set_their_own_axis() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).yscale(Scale::Log).id();
    let a = axes(&fig, id);
    assert_eq!(
        (a.x.scale, a.y.scale, a.z.scale),
        (Scale::Linear, Scale::Log, Scale::Linear)
    );

    fig.axes(0, 0)
        .xscale(Scale::Log)
        .zscale(Scale::Log)
        .yscale(Scale::Linear);
    let a = axes(&fig, id);
    assert_eq!(
        (a.x.scale, a.y.scale, a.z.scale),
        (Scale::Log, Scale::Linear, Scale::Log)
    );
}

// WHY: `grid on` in MATLAB turns on grid lines for every axis; grid(false) turns them
// all off again.
#[test]
fn grid_switches_every_axis() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).grid(true).id();
    let a = axes(&fig, id);
    assert!(a.x.grid && a.y.grid && a.z.grid);

    fig.axes(0, 0).grid(false);
    let a = axes(&fig, id);
    assert!(!a.x.grid && !a.y.grid && !a.z.grid);
}

// WHY: legend() is what makes display names visible and enables click-to-toggle in the
// viewer; it must store the location with the IR's boxed default, and legend_off()
// must remove it.
#[test]
fn legend_is_shown_at_a_location_and_can_be_removed() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).legend(LegendLocation::SouthWest).id();
    assert_eq!(
        axes(&fig, id).legend,
        Some(Legend {
            location: LegendLocation::SouthWest,
            ..Legend::default()
        })
    );

    fig.axes(0, 0).legend_off();
    assert_eq!(axes(&fig, id).legend, None);
}

// WHY: colormap, colour limits and box are axes-wide appearance properties that must map
// to their IR fields.
#[test]
fn colormap_clim_and_box_set_their_fields() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .colormap(Colormap::Coolwarm)
        .clim(-2.0, 2.0)
        .box_on(false)
        .id();
    let a = axes(&fig, id);
    assert_eq!(a.colormap, ColormapName::Coolwarm);
    assert_eq!(
        a.clim,
        Limits::Manual {
            min: -2.0,
            max: 2.0
        }
    );
    assert!(!a.box_);

    fig.axes(0, 0).box_on(true);
    assert!(axes(&fig, id).box_);
}
