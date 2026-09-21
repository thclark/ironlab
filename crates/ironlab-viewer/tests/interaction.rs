//! Pointer gestures as edits recorded in the view overlay, tested without a GPU or a window.
//!
//! Every gesture records a transaction of sets in the overlay and the viewer draws the composition of the source
//! figure with that overlay, so each test reads the displayed figure through [`FigureState::figure`] and the figure as
//! loaded through [`FigureState::source`].

mod common;

use common::*;
use ironlab_ir::{Artist, Dimension, Limits, Line, NodeId, Scale, Text, Value, View3d};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{AxisMap, HitMap, LegendHit};
use ironlab_viewer::interaction::MIN_BOX_ZOOM_POINTS;
use ironlab_viewer::{DATATIP_RADIUS_POINTS, FigureState, Origin, ROTATE_DEGREES_PER_POINT, Tool};

const PLOT: Rect = Rect::new(50.0, 20.0, 200.0, 100.0);

fn single_2d() -> (FigureState, HitMap) {
    let state = FigureState::new(figure_with(vec![axes_2d(2)], vec![]));
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };
    (state, hit)
}

fn single_3d() -> (FigureState, HitMap) {
    let state = FigureState::new(figure_with(vec![axes_3d(2)], vec![]));
    let hit = HitMap {
        axes: vec![hit_3d(2, PLOT)],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };
    (state, hit)
}

/// The axis map that the compiler would produce for the current limits, keeping the figure-space extent of `old`.
fn remapped(old: AxisMap, (min, max): (f64, f64)) -> AxisMap {
    AxisMap { min, max, ..old }
}

/// Drags from `from` to `to` in one update, as one gesture.
fn drag(state: &mut FigureState, hit: &HitMap, from: Point, to: Point) -> bool {
    state.drag_start(hit, from);
    let changed = state.drag_update(to);
    state.drag_end(to) || changed
}

// ---------------------------------------------------------------------------------------------------------------
// Zooming, panning, rotating and box zoom: the behaviour a user relies on, whatever the model behind it
// ---------------------------------------------------------------------------------------------------------------

// Why: zooming about the cursor is what lets a user inspect a feature without it sliding away; if the data point under
// the pointer moved, every wheel notch would need a compensating pan.
#[test]
fn wheel_zoom_keeps_the_data_point_under_the_cursor_on_linear_axes() {
    let (mut state, hit) = single_2d();
    let at = Point::new(100.0, 50.0);
    let (xm, ym) = (x_map(PLOT, 0.0, 10.0, false), y_map(PLOT, 0.0, 5.0, false));
    let (x0, y0) = (xm.to_data(at.x), ym.to_data(at.y));

    assert!(state.scroll(&hit, at, 2.0));

    let x = manual_of(state.figure(), 2, Dimension::X);
    let y = manual_of(state.figure(), 2, Dimension::Y);
    assert_close(x.1 - x.0, 5.0, EPS, "x range halves");
    assert_close(y.1 - y.0, 2.5, EPS, "y range halves");
    assert_close(
        remapped(xm, x).to_data(at.x),
        x0,
        EPS,
        "x data under cursor",
    );
    assert_close(
        remapped(ym, y).to_data(at.y),
        y0,
        EPS,
        "y data under cursor",
    );
}

// Why: scrolling in and then out by the same amount should return the user to where they started; asymmetric zoom
// would make the wheel feel like it drifts.
#[test]
fn wheel_zoom_in_then_out_restores_the_limits() {
    let (mut state, hit) = single_2d();
    let at = Point::new(180.0, 30.0);
    assert!(state.scroll(&hit, at, 1.25));
    let x = manual_of(state.figure(), 2, Dimension::X);
    let y = manual_of(state.figure(), 2, Dimension::Y);
    let zoomed = HitMap {
        axes: vec![ironlab_scene::hit::AxesHit {
            kind: ironlab_scene::hit::AxesHitKind::TwoD {
                x: x_map(PLOT, x.0, x.1, false),
                y: y_map(PLOT, y.0, y.1, false),
            },
            ..hit_2d(2, PLOT)
        }],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };

    assert!(state.scroll(&zoomed, at, 1.0 / 1.25));

    let x = manual_of(state.figure(), 2, Dimension::X);
    let y = manual_of(state.figure(), 2, Dimension::Y);
    assert_close(x.0, 0.0, 1e-9, "x min");
    assert_close(x.1, 10.0, 1e-9, "x max");
    assert_close(y.0, 0.0, 1e-9, "y min");
    assert_close(y.1, 5.0, 1e-9, "y max");
}

// Why: on a log axis the zoom must act on decades, otherwise the point under the cursor jumps and a zoom-out can
// produce a non-positive lower limit, which a log axis cannot show.
#[test]
fn wheel_zoom_keeps_the_data_point_under_the_cursor_on_log_axes() {
    let mut figure = figure_with(vec![axes_2d(2)], vec![]);
    let axes = figure.axes_mut(NodeId(2)).unwrap();
    axes.x.scale = Scale::Log;
    axes.x.limits = manual(1.0, 1e4);
    let mut state = FigureState::new(figure);
    let xm = x_map(PLOT, 1.0, 1e4, true);
    let hit = HitMap {
        axes: vec![ironlab_scene::hit::AxesHit {
            kind: ironlab_scene::hit::AxesHitKind::TwoD {
                x: xm,
                y: y_map(PLOT, 0.0, 5.0, false),
            },
            ..hit_2d(2, PLOT)
        }],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };
    let at = Point::new(100.0, 70.0);
    let x0 = xm.to_data(at.x);

    assert!(state.scroll(&hit, at, 2.0));
    let x = manual_of(state.figure(), 2, Dimension::X);
    assert_close(x.1.log10() - x.0.log10(), 2.0, 1e-9, "decades shown halve");
    assert_close(
        remapped(xm, x).to_data(at.x),
        x0,
        1e-9 * x0,
        "x data under cursor",
    );

    let zoomed = HitMap {
        axes: vec![ironlab_scene::hit::AxesHit {
            kind: ironlab_scene::hit::AxesHitKind::TwoD {
                x: remapped(xm, x),
                y: y_map(PLOT, 0.0, 5.0, false),
            },
            ..hit_2d(2, PLOT)
        }],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };
    assert!(state.scroll(&zoomed, at, 0.1));
    let x = manual_of(state.figure(), 2, Dimension::X);
    assert!(
        x.0 > 0.0,
        "log limits stay positive after zooming out, got {x:?}"
    );
}

// Why: automatic limits are recomputed from the data on every compile, so a zoom that left them Auto would be undone
// immediately; the zoom must start from the limits the user was looking at and pin them, while the source figure keeps
// its automatic limits so that resetting the view makes them automatic again.
#[test]
fn wheel_zoom_on_automatic_limits_writes_manual_limits_from_the_resolved_ones() {
    let mut figure = figure_with(vec![axes_2d(2)], vec![]);
    let axes = figure.axes_mut(NodeId(2)).unwrap();
    axes.x.limits = Limits::Auto;
    axes.y.limits = Limits::Auto;
    let mut state = FigureState::new(figure);
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };
    let centre = Point::new(PLOT.x + PLOT.width / 2.0, PLOT.y + PLOT.height / 2.0);

    assert!(state.scroll(&hit, centre, 2.0));

    let x = manual_of(state.figure(), 2, Dimension::X);
    let y = manual_of(state.figure(), 2, Dimension::Y);
    assert_close(x.0, 2.5, EPS, "x min");
    assert_close(x.1, 7.5, EPS, "x max");
    assert_close(y.0, 1.25, EPS, "y min");
    assert_close(y.1, 3.75, EPS, "y max");
    assert_eq!(
        limits_of(state.source(), 2, Dimension::X),
        Limits::Auto,
        "the source keeps its automatic limits"
    );
}

// Why: a grabbed point must stay under the pointer for the whole drag; computing each update incrementally from the
// previous one accumulates floating-point error over the hundreds of pointer events in a long drag.
#[test]
fn pan_follows_the_pointer_without_drift_over_many_updates() {
    let (mut state, hit) = single_2d();
    let (xm, ym) = (x_map(PLOT, 0.0, 10.0, false), y_map(PLOT, 0.0, 5.0, false));
    let start = Point::new(100.0, 50.0);
    let end = Point::new(183.7, 91.3);
    let (grab_x, grab_y) = (xm.to_data(start.x), ym.to_data(start.y));

    state.drag_start(&hit, start);
    let steps = 997;
    for i in 1..=steps {
        let t = f64::from(i) / f64::from(steps);
        let at = Point::new(
            start.x + t * (end.x - start.x),
            start.y + t * (end.y - start.y),
        );
        state.drag_update(at);
    }
    let x = manual_of(state.figure(), 2, Dimension::X);
    let y = manual_of(state.figure(), 2, Dimension::Y);

    assert_close(
        remapped(xm, x).to_figure(grab_x),
        end.x,
        1e-9,
        "grabbed x under pointer",
    );
    assert_close(
        remapped(ym, y).to_figure(grab_y),
        end.y,
        1e-9,
        "grabbed y under pointer",
    );
    assert_close(x.1 - x.0, 10.0, 1e-9, "pan keeps the x range");

    let (mut direct, hit) = single_2d();
    direct.drag_start(&hit, start);
    assert!(direct.drag_update(end));
    assert_eq!(
        (x, y),
        (
            manual_of(direct.figure(), 2, Dimension::X),
            manual_of(direct.figure(), 2, Dimension::Y)
        ),
        "many small updates give exactly the same limits as one update to the same point"
    );

    state.drag_end(end);
    assert_eq!(
        manual_of(state.figure(), 2, Dimension::X),
        x,
        "releasing does not move the view"
    );
}

// Why: a drag that returns to where it began must leave the view where it began; otherwise a user who changes their
// mind mid-drag cannot get back without resetting.
#[test]
fn a_pan_that_returns_to_its_start_restores_the_limits_it_started_from() {
    let (mut state, hit) = single_2d();
    let start = Point::new(100.0, 50.0);

    state.drag_start(&hit, start);
    assert!(state.drag_update(Point::new(160.0, 90.0)));
    assert!(state.drag_update(start));
    state.drag_end(start);

    assert_close(
        manual_of(state.figure(), 2, Dimension::X).0,
        0.0,
        1e-9,
        "x min",
    );
    assert_close(
        manual_of(state.figure(), 2, Dimension::Y).1,
        5.0,
        1e-9,
        "y max",
    );
}

// Why: panning a log axis must shift it by a constant number of decades so that the grabbed value, not its linear
// offset, stays under the pointer.
#[test]
fn pan_on_a_log_axis_keeps_the_grabbed_value_under_the_pointer() {
    let mut figure = figure_with(vec![axes_2d(2)], vec![]);
    let axes = figure.axes_mut(NodeId(2)).unwrap();
    axes.y.scale = Scale::Log;
    axes.y.limits = manual(1e-2, 1e2);
    let mut state = FigureState::new(figure);
    let ym = y_map(PLOT, 1e-2, 1e2, true);
    let hit = HitMap {
        axes: vec![ironlab_scene::hit::AxesHit {
            kind: ironlab_scene::hit::AxesHitKind::TwoD {
                x: x_map(PLOT, 0.0, 10.0, false),
                y: ym,
            },
            ..hit_2d(2, PLOT)
        }],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };
    let start = Point::new(120.0, 90.0);
    let end = Point::new(120.0, 40.0);
    let grab = ym.to_data(start.y);

    state.drag_start(&hit, start);
    assert!(state.drag_update(end));

    let y = manual_of(state.figure(), 2, Dimension::Y);
    assert_close(
        y.1.log10() - y.0.log10(),
        4.0,
        1e-9,
        "pan keeps four decades",
    );
    assert_close(
        remapped(ym, y).to_figure(grab),
        end.y,
        1e-9,
        "grabbed value under pointer",
    );
}

// Why: linking is per dimension; a pan in one subplot must move exactly the partners linked in that dimension, leave
// their other dimensions alone, and leave unlinked subplots untouched.
#[test]
fn pan_on_x_linked_axes_moves_the_partner_x_only_and_leaves_unlinked_axes_alone() {
    let rect_a = Rect::new(20.0, 20.0, 100.0, 80.0);
    let rect_b = Rect::new(140.0, 20.0, 100.0, 80.0);
    let rect_c = Rect::new(260.0, 20.0, 100.0, 80.0);
    let mut b = axes_2d(3);
    b.y.limits = manual(-1.0, 1.0);
    let figure = figure_with(
        vec![axes_2d(2), b, axes_2d(4)],
        vec![link(Dimension::X, &[2, 3])],
    );
    let before = figure.clone();
    let mut state = FigureState::new(figure);
    let hit = HitMap {
        axes: vec![
            hit_2d(2, rect_a),
            ironlab_scene::hit::AxesHit {
                kind: ironlab_scene::hit::AxesHitKind::TwoD {
                    x: x_map(rect_b, 0.0, 10.0, false),
                    y: y_map(rect_b, -1.0, 1.0, false),
                },
                ..hit_2d(3, rect_b)
            },
            hit_2d(4, rect_c),
        ],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };

    state.drag_start(&hit, Point::new(60.0, 60.0));
    assert!(state.drag_update(Point::new(100.0, 80.0)));

    let a_x = manual_of(state.figure(), 2, Dimension::X);
    assert_ne!(a_x, (0.0, 10.0), "the dragged axes moved in x");
    assert_ne!(
        manual_of(state.figure(), 2, Dimension::Y),
        (0.0, 5.0),
        "the dragged axes moved in y"
    );
    assert_eq!(
        manual_of(state.figure(), 3, Dimension::X),
        a_x,
        "the x-linked partner follows in x"
    );
    assert_eq!(
        limits_of(state.figure(), 3, Dimension::Y),
        limits_of(&before, 3, Dimension::Y),
        "the partner is not linked in y"
    );
    assert_eq!(
        state.figure().axes(NodeId(4)),
        before.axes(NodeId(4)),
        "the unlinked axes is untouched"
    );
}

// Why: the rotate gesture must feel like grabbing the object (MATLAB rotate3d), with a documented rate, and must never
// tip the camera past the poles, where the view would flip upside down.
#[test]
fn rotate_drag_changes_azimuth_and_elevation_with_the_documented_sign_and_clamps_elevation() {
    let (mut state, hit) = single_3d();
    state.tool = Tool::Rotate;
    let before = state.figure().clone();
    let start = Point::new(150.0, 70.0);

    state.drag_start(&hit, start);
    assert!(state.drag_update(Point::new(start.x + 10.0, start.y + 4.0)));
    let view = view_of(state.figure(), 2);
    assert_close(
        view.azimuth_deg,
        -37.5 - 10.0 * ROTATE_DEGREES_PER_POINT,
        EPS,
        "dragging right decreases azimuth",
    );
    assert_close(
        view.elevation_deg,
        30.0 + 4.0 * ROTATE_DEGREES_PER_POINT,
        EPS,
        "dragging down increases elevation",
    );

    state.drag_update(Point::new(start.x, start.y + 1000.0));
    assert_close(
        view_of(state.figure(), 2).elevation_deg,
        90.0,
        EPS,
        "elevation clamps at +90",
    );
    state.drag_update(Point::new(start.x, start.y - 1000.0));
    assert_close(
        view_of(state.figure(), 2).elevation_deg,
        -90.0,
        EPS,
        "elevation clamps at -90",
    );

    state.drag_end(Point::new(start.x, start.y - 1000.0));
    let axes = state.figure().axes(NodeId(2)).unwrap();
    let old = before.axes(NodeId(2)).unwrap();
    assert_eq!(
        (&axes.x, &axes.y, &axes.z),
        (&old.x, &old.y, &old.z),
        "rotation never changes limits"
    );
}

// Why: 3D axes are navigated through their camera; changing data limits on wheel zoom would rescale the box and move
// ticks instead of magnifying the view.
#[test]
fn wheel_zoom_on_3d_axes_changes_the_camera_zoom_not_the_limits() {
    let (mut state, hit) = single_3d();
    let before = state.figure().clone();

    assert!(state.scroll(&hit, Point::new(150.0, 70.0), 2.0));

    assert_close(view_of(state.figure(), 2).zoom, 2.0, EPS, "zoom doubles");
    let axes = state.figure().axes(NodeId(2)).unwrap();
    let old = before.axes(NodeId(2)).unwrap();
    assert_eq!((&axes.x, &axes.y, &axes.z), (&old.x, &old.y, &old.z));
}

// Why: panning a 3D axes moves the camera offset, which the scene compiler interprets as fractions of the plot
// rectangle; the direction contract (positive pan_y is downwards) must match the compiler for the box to follow the
// pointer.
#[test]
fn pan_on_3d_axes_moves_the_view_offset_by_the_pointer_displacement_in_plot_fractions() {
    let (mut state, hit) = single_3d();
    let before = state.figure().clone();

    state.drag_start(&hit, Point::new(100.0, 50.0));
    assert!(state.drag_update(Point::new(120.0, 60.0)));

    let view = view_of(state.figure(), 2);
    assert_close(view.pan_x, 20.0 / PLOT.width, EPS, "pan x");
    assert_close(view.pan_y, 10.0 / PLOT.height, EPS, "pan y");
    let axes = state.figure().axes(NodeId(2)).unwrap();
    assert_eq!(
        axes.x,
        before.axes(NodeId(2)).unwrap().x,
        "3D pan does not change limits"
    );
}

// Why: box zoom is the precise way to select a region; the resulting limits must be exactly the data covered by the
// band, whichever corner the user started from, and the band must be visible while dragging.
#[test]
fn box_zoom_sets_the_limits_to_the_data_covered_by_the_rubber_band() {
    for (from, to) in [
        (Point::new(100.0, 40.0), Point::new(150.0, 90.0)),
        (Point::new(150.0, 90.0), Point::new(100.0, 40.0)),
    ] {
        let (mut state, hit) = single_2d();
        state.tool = Tool::Zoom;

        state.drag_start(&hit, from);
        assert!(
            !state.drag_update(to),
            "the zoom tool does not change the figure while dragging"
        );
        assert_eq!(
            state.rubber_band(),
            Some(Rect::new(100.0, 40.0, 50.0, 50.0))
        );
        assert!(state.drag_end(to));

        assert_eq!(state.rubber_band(), None, "the band disappears on release");
        let x = manual_of(state.figure(), 2, Dimension::X);
        let y = manual_of(state.figure(), 2, Dimension::Y);
        assert_close(x.0, 2.5, EPS, "x min");
        assert_close(x.1, 5.0, EPS, "x max");
        assert_close(y.0, 1.5, EPS, "y min");
        assert_close(y.1, 4.0, EPS, "y max");
    }
}

// Why: users routinely release a rubber band beyond the edge of the axes; the band must stop at the plot rectangle
// so that the zoom never extends past the data region the user could see.
#[test]
fn box_zoom_clamps_the_band_to_the_plot_rectangle() {
    let (mut state, hit) = single_2d();
    state.tool = Tool::Zoom;

    state.drag_start(&hit, Point::new(200.0, 100.0));
    state.drag_update(Point::new(400.0, 300.0));
    assert_eq!(
        state.rubber_band(),
        Some(Rect::new(200.0, 100.0, 50.0, 20.0)),
        "the band stops at the bottom-right corner of the plot rectangle"
    );
    assert!(state.drag_end(Point::new(400.0, 300.0)));

    let x = manual_of(state.figure(), 2, Dimension::X);
    let y = manual_of(state.figure(), 2, Dimension::Y);
    assert_close(x.0, 7.5, EPS, "x min");
    assert_close(x.1, 10.0, EPS, "x max is the old limit, not beyond it");
    assert_close(y.0, 0.0, EPS, "y min is the old limit, not beyond it");
    assert_close(y.1, 1.0, EPS, "y max");
}

// Why: a 3D axes has no data mapping for a rubber band to select, so the Zoom tool must not edit a 3D axes on drag
// (nor draw a band that suggests it will); the wheel is the zoom gesture that works on every axes, whichever drag tool
// is active.
#[test]
fn the_zoom_tool_does_not_drag_on_3d_axes_and_the_wheel_zooms_in_every_tool() {
    let (mut state, hit) = single_3d();
    state.tool = Tool::Zoom;
    let before = state.figure().clone();

    state.drag_start(&hit, Point::new(100.0, 40.0));
    assert!(!state.drag_update(Point::new(150.0, 90.0)));
    assert_eq!(state.rubber_band(), None, "no band is drawn over 3D axes");
    assert!(!state.drag_end(Point::new(150.0, 90.0)));
    assert_eq!(state.figure(), &before);

    for tool in [Tool::Pan, Tool::Zoom, Tool::Rotate] {
        let (mut state, hit) = single_3d();
        state.tool = tool;
        assert!(state.scroll(&hit, Point::new(150.0, 70.0), 2.0), "{tool:?}");
        assert_close(view_of(state.figure(), 2).zoom, 2.0, EPS, "3D zoom");

        let (mut state, hit) = single_2d();
        state.tool = tool;
        assert!(state.scroll(&hit, Point::new(150.0, 70.0), 2.0), "{tool:?}");
        let x = manual_of(state.figure(), 2, Dimension::X);
        assert_close(x.1 - x.0, 5.0, EPS, "2D zoom");
    }
}

// Why: a click or a slip of the mouse with the Zoom tool would otherwise zoom into a sliver with nearly equal limits,
// which is disorienting and can make the axes degenerate.
#[test]
fn box_zoom_ignores_bands_smaller_than_the_minimum_size() {
    let small = MIN_BOX_ZOOM_POINTS - 1.0;
    for to in [
        Point::new(100.0 + small, 40.0 + small),
        Point::new(100.0 + small, 90.0),
        Point::new(150.0, 40.0 + small),
    ] {
        let (mut state, hit) = single_2d();
        state.tool = Tool::Zoom;
        let before = state.figure().clone();

        state.drag_start(&hit, Point::new(100.0, 40.0));
        state.drag_update(to);
        assert!(!state.drag_end(to), "band to {to:?} is ignored");
        assert_eq!(state.figure(), &before);
        assert!(!state.can_undo(), "an ignored band leaves nothing to undo");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Restoring views
// ---------------------------------------------------------------------------------------------------------------

// Why: double-click is the local undo for one subplot; it must not throw away navigation in other subplots, but must
// keep linked partners consistent with the axes it resets.
#[test]
fn double_click_restores_only_the_axes_under_the_pointer_and_its_linked_axes() {
    let rect_a = Rect::new(20.0, 20.0, 100.0, 80.0);
    let rect_b = Rect::new(140.0, 20.0, 100.0, 80.0);
    let rect_c = Rect::new(260.0, 20.0, 100.0, 80.0);
    let figure = figure_with(
        vec![axes_2d(2), axes_2d(3), axes_2d(4)],
        vec![link(Dimension::X, &[2, 3])],
    );
    let mut state = FigureState::new(figure);
    let hit = HitMap {
        axes: vec![hit_2d(2, rect_a), hit_2d(3, rect_b), hit_2d(4, rect_c)],
        legend_entries: vec![],
        artists: vec![],
        images: vec![],
    };
    for (id, x, y) in [
        (2, (100.0, 110.0), (7.0, 8.0)),
        (3, (100.0, 110.0), (50.0, 60.0)),
        (4, (-3.0, 3.0), (-1.0, 1.0)),
    ] {
        record_limits(&mut state, id, Dimension::X, manual(x.0, x.1));
        record_limits(&mut state, id, Dimension::Y, manual(y.0, y.1));
    }

    assert!(state.double_click(&hit, Point::new(300.0, 60.0)));
    assert_eq!(manual_of(state.figure(), 4, Dimension::X), (0.0, 10.0));
    assert_eq!(manual_of(state.figure(), 4, Dimension::Y), (0.0, 5.0));
    assert_eq!(
        manual_of(state.figure(), 2, Dimension::X),
        (100.0, 110.0),
        "other axes keep their view"
    );

    assert!(state.double_click(&hit, Point::new(60.0, 60.0)));
    assert_eq!(manual_of(state.figure(), 2, Dimension::X), (0.0, 10.0));
    assert_eq!(manual_of(state.figure(), 2, Dimension::Y), (0.0, 5.0));
    assert_eq!(
        manual_of(state.figure(), 3, Dimension::X),
        (0.0, 10.0),
        "x-linked partner follows"
    );
    assert_eq!(
        manual_of(state.figure(), 3, Dimension::Y),
        (50.0, 60.0),
        "partner is not linked in y"
    );
}

// Why: double-click on a rotated 3D axes is how a user gets back to the as-loaded camera without resetting every
// subplot.
#[test]
fn double_click_restores_the_view_of_a_3d_axes() {
    let (mut state, hit) = single_3d();
    let rotated = View3d {
        azimuth_deg: 10.0,
        elevation_deg: -20.0,
        zoom: 3.0,
        pan_x: 0.2,
        pan_y: -0.1,
    };
    assert!(state.record(&set(2, "projection.view3d", Value::View3d(rotated))));

    assert!(state.double_click(&hit, Point::new(150.0, 70.0)));
    assert_eq!(view_of(state.figure(), 2), view_of(state.source(), 2));
}

// Why: Reset view undoes navigation, not content choices; a user who hid a series should not see it reappear because
// they reset the zoom. The changed flag drives recompilation, so a no-op reset must report no change.
#[test]
fn reset_view_restores_limits_and_views_but_keeps_visibility() {
    let mut axes = axes_2d(2);
    axes.artists.push(Artist::Line(Line {
        id: NodeId(10),
        ..Line::default()
    }));
    let mut state = FigureState::new(figure_with(vec![axes, axes_3d(3)], vec![]));
    let source = state.source().clone();
    record_limits(&mut state, 2, Dimension::X, manual(3.0, 4.0));
    record_limits(&mut state, 3, Dimension::Z, manual(-9.0, 9.0));
    state.record(&set(
        3,
        "projection.view3d",
        Value::View3d(View3d {
            azimuth_deg: 45.0,
            elevation_deg: 10.0,
            zoom: 0.5,
            pan_x: 0.1,
            pan_y: 0.1,
        }),
    ));
    state.record(&set(10, "visible", Value::Bool(false)));

    assert!(state.reset_view());

    for id in [2, 3] {
        for dim in [Dimension::X, Dimension::Y, Dimension::Z] {
            assert_eq!(
                limits_of(state.figure(), id, dim),
                limits_of(&source, id, dim),
                "axes {id} {dim:?}"
            );
        }
    }
    assert_eq!(view_of(state.figure(), 3), view_of(&source, 3));
    assert!(!visible(&state, 10), "visibility is kept");
    assert!(
        !state.reset_view(),
        "resetting an already reset view changes nothing"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// The legend
// ---------------------------------------------------------------------------------------------------------------

fn figure_with_legend() -> (FigureState, HitMap) {
    let mut axes = axes_2d(2);
    axes.artists.push(Artist::Line(Line {
        id: NodeId(10),
        display_name: Some(Text::new("series")),
        ..Line::default()
    }));
    let state = FigureState::new(figure_with(vec![axes], vec![]));
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![LegendHit {
            axes: NodeId(2),
            artist: NodeId(10),
            rect: Rect::new(200.0, 25.0, 45.0, 10.0),
        }],
        artists: vec![],
        images: vec![],
    };
    (state, hit)
}

fn visible(state: &FigureState, id: u64) -> bool {
    state.figure().artist(NodeId(id)).unwrap().1.visible()
}

// Why: clicking a legend entry is the MATLAB-style way to hide and re-show a series; it must be a clean toggle, and a
// click elsewhere in the plot must not hide anything.
#[test]
fn clicking_a_legend_entry_toggles_its_artist_and_other_clicks_do_nothing() {
    let (mut state, hit) = figure_with_legend();

    assert!(state.click(&hit, Point::new(220.0, 30.0)));
    assert!(!visible(&state, 10));
    assert!(state.click(&hit, Point::new(220.0, 30.0)));
    assert!(visible(&state, 10));

    assert!(
        !state.click(&hit, Point::new(100.0, 100.0)),
        "a click in the plot area is not a legend click"
    );
    assert!(visible(&state, 10));
}

// Why: the viewer reports the first click of a double-click as a click and the second as a double-click. On a legend
// entry each click is a toggle, so a quick double-click must act as two toggles and leave the series as it was; it must
// not also restore the axes limits, which would silently throw away the user's navigation.
#[test]
fn double_clicking_a_legend_entry_toggles_twice_and_keeps_the_limits() {
    let (mut state, hit) = figure_with_legend();
    let navigated = manual(2.0, 4.0);
    record_limits(&mut state, 2, Dimension::X, navigated);
    let entry = Point::new(220.0, 30.0);

    assert!(state.click(&hit, entry), "the first click hides the series");
    assert!(!visible(&state, 10));
    assert!(
        state.double_click(&hit, entry),
        "the second click shows it again"
    );
    assert!(visible(&state, 10));
    assert_eq!(
        limits_of(state.figure(), 2, Dimension::X),
        navigated,
        "a double-click on a legend entry does not reset the axes"
    );

    assert!(
        state.double_click(&hit, Point::new(100.0, 100.0)),
        "a double-click in the plot area still resets the axes"
    );
    assert_ne!(limits_of(state.figure(), 2, Dimension::X), navigated);
}

// ---------------------------------------------------------------------------------------------------------------
// The source, the overlay and the displayed figure
// ---------------------------------------------------------------------------------------------------------------

// Why: the overlay only makes sense if the figure the viewer was given is never touched; the property editor, saving
// and a future session all rely on the source being the owner's figure and on the user's changes being values that
// can be dropped, reverted or sent elsewhere.
#[test]
fn no_gesture_changes_the_source_figure() {
    let (mut state, hit) = figure_with_legend();
    let source = state.source().clone();

    state.scroll(&hit, Point::new(100.0, 50.0), 2.0);
    drag(
        &mut state,
        &hit,
        Point::new(100.0, 50.0),
        Point::new(140.0, 70.0),
    );
    state.click(&hit, Point::new(220.0, 30.0));
    state.double_click(&hit, Point::new(100.0, 50.0));

    assert_ne!(
        state.figure(),
        &source,
        "precondition: the gestures changed the displayed figure"
    );
    assert_eq!(state.source(), &source, "the source is untouched");

    let (mut state, hit) = single_3d();
    let source = state.source().clone();
    state.tool = Tool::Rotate;
    drag(
        &mut state,
        &hit,
        Point::new(150.0, 70.0),
        Point::new(180.0, 90.0),
    );
    assert_ne!(
        state.figure(),
        &source,
        "precondition: the rotation applied"
    );
    assert_eq!(state.source(), &source, "the source is untouched");
}

// Why: a drag sends hundreds of pointer events; if each were an undo step the user would have to press undo hundreds
// of times to get back to where they started, and the redo history would be equally unusable.
#[test]
fn a_drag_is_one_undo_step_whatever_the_number_of_pointer_events() {
    let (mut state, hit) = single_2d();
    let start = Point::new(100.0, 50.0);

    state.drag_start(&hit, start);
    for i in 1..=20 {
        state.drag_update(Point::new(start.x + f64::from(i), start.y + f64::from(i)));
    }
    assert!(
        !state.can_undo(),
        "the step is still open while the drag continues"
    );
    state.drag_end(Point::new(start.x + 20.0, start.y + 20.0));

    assert!(state.can_undo());
    assert!(state.undo(), "one undo returns to the start of the drag");
    assert_eq!(manual_of(state.figure(), 2, Dimension::X), (0.0, 10.0));
    assert_eq!(manual_of(state.figure(), 2, Dimension::Y), (0.0, 5.0));
    assert!(!state.can_undo(), "the whole drag was one step");
}

// Why: undo and redo are only useful if they are exact inverses of each other, and if each step is a gesture: a user
// who scrolls, pans and hides a series expects three undos to return to the figure as opened, and three redos to
// return to what they had.
#[test]
fn undo_and_redo_walk_gesture_by_gesture_between_the_figure_as_opened_and_the_latest_view() {
    let (mut state, hit) = figure_with_legend();
    let opened = state.figure().clone();

    state.scroll(&hit, Point::new(100.0, 50.0), 2.0);
    drag(
        &mut state,
        &hit,
        Point::new(100.0, 50.0),
        Point::new(140.0, 70.0),
    );
    state.click(&hit, Point::new(220.0, 30.0));
    let latest = state.figure().clone();

    assert!(state.undo(), "the legend click");
    assert!(visible(&state, 10), "undoing the click shows the series");
    assert!(state.undo(), "the pan");
    assert!(state.undo(), "the wheel notch");
    assert_eq!(
        state.figure(),
        &opened,
        "three undos reach the figure as opened"
    );
    assert!(!state.can_undo());

    assert!(state.redo());
    assert!(state.redo());
    assert!(state.redo());
    assert_eq!(state.figure(), &latest, "three redos reach the latest view");
    assert!(!state.can_redo());
    assert!(!state.redo(), "redo at the end of the history does nothing");
}

// Why: redo after a new gesture would re-apply a change that the user has already replaced, silently overwriting the
// branch they chose; the redo history must be dropped as soon as they navigate again.
#[test]
fn a_gesture_after_an_undo_clears_the_redo_history() {
    let (mut state, hit) = single_2d();
    state.scroll(&hit, Point::new(100.0, 50.0), 2.0);
    assert!(state.undo());
    assert!(state.can_redo());

    drag(
        &mut state,
        &hit,
        Point::new(100.0, 50.0),
        Point::new(140.0, 70.0),
    );

    assert!(!state.can_redo(), "the new drag replaced the undone zoom");
    assert!(!state.redo());
}

// Why: Reset view and double-click throw away every view change at once, which is exactly the action a user is most
// likely to regret; each must be one undo step of its own.
#[test]
fn resetting_a_view_is_one_undo_step() {
    let (mut state, hit) = single_2d();
    state.scroll(&hit, Point::new(100.0, 50.0), 2.0);
    let zoomed = state.figure().clone();

    assert!(state.reset_view());
    assert!(state.undo(), "the reset is undone");
    assert_eq!(state.figure(), &zoomed, "the zoom comes back");

    assert!(state.double_click(&hit, Point::new(100.0, 50.0)));
    assert!(state.undo(), "the double-click is one step");
    assert_eq!(state.figure(), &zoomed);
}

// Why: saving writes the composed figure, so the file and what the user sees must agree; and because the viewer then
// owns that figure as its source, the changes it has just written must no longer be pending in the overlay.
#[test]
fn folding_the_overlay_moves_the_displayed_figure_into_the_source_and_empties_the_overlay() {
    let (mut state, hit) = single_2d();
    state.scroll(&hit, Point::new(100.0, 50.0), 2.0);
    let displayed = state.figure().clone();
    assert!(!state.overlay().entries().is_empty());

    state.fold_overlay();

    assert_eq!(state.source(), &displayed, "the source is what was written");
    assert_eq!(state.figure(), &displayed, "the display does not move");
    assert!(state.overlay().entries().is_empty());
    assert!(
        !state.can_undo() && !state.can_redo(),
        "the folded changes are the source now, so they cannot be undone"
    );
    assert!(
        !state.reset_view(),
        "resetting restores the figure as saved, which is what is shown"
    );
}

// Why: an entry that the source cannot accept would otherwise be applied on every recomposition and silently ignored,
// leaving the user looking at a figure that does not match what they asked for with no explanation.
#[test]
fn an_entry_that_the_figure_cannot_show_is_discarded_and_reported_as_a_problem() {
    let (mut state, _) = single_2d();
    assert!(state.problems().is_empty());

    assert!(!state.record(&set(2, "x.limits", Value::Limits(manual(4.0, 4.0)))));

    assert_eq!(manual_of(state.figure(), 2, Dimension::X), (0.0, 10.0));
    assert!(
        state.overlay().entries().is_empty(),
        "the entry is discarded rather than retried on every recomposition"
    );
    let problems = state.problems();
    assert_eq!(problems.len(), 1, "one problem: {problems:?}");
    assert_eq!(problems[0].origin, Origin::Discarded);
    assert_eq!(problems[0].node, Some(NodeId(2)));
    assert_eq!(
        problems[0].path.as_ref().map(ToString::to_string),
        Some("x.limits".to_owned()),
        "the problem names the property it concerned"
    );
    assert!(
        !state.can_undo(),
        "a change that was discarded leaves no undo step, which would undo to the same figure"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Events that must do nothing
// ---------------------------------------------------------------------------------------------------------------

// Why: the canvas forwards every pointer event; events over margins, titles or empty figure space must not edit the
// figure or trigger a recompile.
#[test]
fn events_outside_every_axes_do_nothing_and_report_no_change() {
    let (mut state, hit) = figure_with_legend();
    let outside = Point::new(5.0, 5.0);
    let before = state.figure().clone();

    for tool in [Tool::Pan, Tool::Zoom, Tool::Rotate] {
        state.tool = tool;
        assert!(!state.scroll(&hit, outside, 2.0));
        state.drag_start(&hit, outside);
        assert!(!state.drag_update(Point::new(100.0, 50.0)));
        assert_eq!(state.rubber_band(), None);
        assert!(!state.drag_end(Point::new(100.0, 50.0)));
        assert!(!state.double_click(&hit, outside));
        assert!(!state.click(&hit, outside));
    }
    assert!(
        !state.drag_update(Point::new(10.0, 10.0)),
        "an update without a drag does nothing"
    );
    assert_eq!(state.figure(), &before);
    assert!(
        !state.can_undo(),
        "nothing that changed nothing is on the undo history"
    );
}

// Why: the toolbar enables Rotate only for figures that have something to rotate.
#[test]
fn has_3d_reports_whether_any_axes_is_three_dimensional() {
    assert!(!single_2d().0.has_3d());
    assert!(single_3d().0.has_3d());
    assert!(FigureState::new(figure_with(vec![axes_2d(2), axes_3d(3)], vec![])).has_3d());
}

// Why: a 2D axes has no camera, so a Rotate-tool drag over it must leave the figure alone (and report no change, so
// that the canvas does not recompile) rather than, for example, falling back to a pan the user did not ask for.
#[test]
fn a_rotate_drag_on_2d_axes_does_nothing() {
    let (mut state, hit) = single_2d();
    state.tool = Tool::Rotate;
    let before = state.figure().clone();

    state.drag_start(&hit, Point::new(100.0, 50.0));
    assert!(!state.drag_update(Point::new(160.0, 90.0)));
    assert!(!state.drag_end(Point::new(160.0, 90.0)));

    assert_eq!(state.figure(), &before);
    assert!(!state.can_undo());
}

// ---------------------------------------------------------------------------------------------------------------
// Datatips
// ---------------------------------------------------------------------------------------------------------------
//
// These tests compile the figure rather than building a hit map by hand, because what they are about is the
// agreement between the points the scene compiler drew and the data the viewer reports for them.

/// A figure of one axes holding a line of `n` points, with automatic limits so that the whole series is in view.
///
/// Returns the state and the x and y arrays, whose values differ at every index so that a reported value identifies
/// the index it was read from.
fn dense_line(n: usize) -> (FigureState, Vec<f64>, Vec<f64>) {
    use ironlab_ir::{Axes, DataId, Figure, NdArray};

    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let y: Vec<f64> = (0..n)
        .map(|i| (i as f64 / 250.0).sin() + i as f64 / 1e6)
        .collect();
    let (xi, yi) = (DataId(0), DataId(1));
    let figure = Figure {
        id: NodeId(1),
        data: std::collections::BTreeMap::from([
            (xi, NdArray::vector(x.clone())),
            (yi, NdArray::vector(y.clone())),
        ]),
        axes: vec![Axes {
            id: NodeId(2),
            artists: vec![Artist::Line(Line {
                id: NodeId(3),
                display_name: Some(Text::plain("Measured")),
                x: xi,
                y: yi,
                ..Line::default()
            })],
            ..Axes::default()
        }],
        ..Figure::new()
    };
    (FigureState::new(figure), x, y)
}

// Why: this is what the index map is for. A series too dense to draw in full is drawn from a subset of its points,
// and a datatip that reported a position in that subset would name a different measurement every time the reader
// zoomed. The index must be the one the user's own arrays use, and the values must be read from those arrays at
// that index.
#[test]
fn a_datatip_on_a_decimated_series_reports_the_index_and_values_of_the_original_data() {
    let n = 80_000;
    let (state, x, y) = dense_line(n);
    let scene = ironlab_scene::compile(state.figure(), &TEXT);
    let drawn = &scene.hit_map.artists[0];
    assert!(
        drawn.samples.len() < n / 10,
        "the series was decimated: {} of {n} points drawn",
        drawn.samples.len()
    );

    let position_in_the_drawn_series = 500;
    let sample = drawn.samples[position_in_the_drawn_series];
    let tip = state
        .datatip(&scene.hit_map, sample.position)
        .expect("a drawn point lies under its own position");

    assert_eq!(tip.artist, NodeId(3));
    assert_eq!(tip.axes, NodeId(2));
    assert_eq!(tip.name.as_deref(), Some("Measured"));
    assert_ne!(
        tip.index, position_in_the_drawn_series,
        "the index reported is the one in the source data, not the place in the drawn series"
    );
    assert!(tip.index < n);
    assert_eq!(tip.x, x[tip.index]);
    assert_eq!(tip.y, y[tip.index]);
    assert_eq!(tip.z, None);
}

// Why: the pointer is never exactly on a point, and a datatip that appeared for any position inside the axes would
// name a measurement the reader is not pointing at, so reading is limited to the neighbourhood of a drawn point.
#[test]
fn a_datatip_is_read_only_near_a_point_that_was_drawn() {
    let (state, _, _) = dense_line(80_000);
    let scene = ironlab_scene::compile(state.figure(), &TEXT);
    let sample = scene.hit_map.artists[0].samples[100];
    let just_inside = Point::new(
        sample.position.x,
        sample.position.y + DATATIP_RADIUS_POINTS - 0.5,
    );
    // The top-left corner of the page, which is outside every axes.
    let well_away = Point::new(2.0, 2.0);

    assert!(state.datatip(&scene.hit_map, just_inside).is_some());
    assert!(state.datatip(&scene.hit_map, well_away).is_none());
}

// Why: decimation is redone for every view, so the points that are drawn — and so the points a datatip can read —
// change as the reader zooms. Zooming in must give access to measurements that were thinned away before, or the
// datatip would stay at the resolution of the first view however far the reader zoomed.
#[test]
fn zooming_in_lets_a_datatip_read_points_that_were_thinned_away() {
    let (mut state, _, _) = dense_line(80_000);
    let readable = |state: &FigureState| {
        let scene = ironlab_scene::compile(state.figure(), &TEXT);
        scene.hit_map.artists[0]
            .samples
            .iter()
            .map(|s| s.source_index)
            .collect::<std::collections::BTreeSet<usize>>()
    };
    let before = readable(&state);
    record_limits(&mut state, 2, Dimension::X, manual(40_000.0, 40_500.0));
    let after = readable(&state);

    let window = 40_000..=40_500;
    let in_window = |set: &std::collections::BTreeSet<usize>| {
        set.iter().filter(|i| window.contains(*i)).count()
    };
    assert!(
        in_window(&after) > in_window(&before),
        "the zoomed view can read {} points of the window against {}",
        in_window(&after),
        in_window(&before)
    );
}
