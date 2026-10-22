//! Pointer gestures as edits recorded in the view overlay, tested without a GPU or a window.
//!
//! Every gesture records a transaction of sets in the overlay and the viewer draws the composition of the source
//! figure with that overlay, so each test reads the displayed figure through [`FigureState::figure`] and the figure as
//! loaded through [`FigureState::source`].

mod common;

use common::*;
use ironlab_canvas::interaction::{MIN_BOX_ZOOM_POINTS, PixelDatatip, PixelValue, Tip};
use ironlab_canvas::{DATATIP_RADIUS_POINTS, FigureState, Origin, ROTATE_DEGREES_PER_POINT, Tool};
use std::f64::consts::TAU;

use ironlab_ir::{
    Artist, Aspect, AspectAdjust, Axes, Color, ColorScaleKind, Coords, Dimension, Figure, Image,
    ImagePlacement, Limits, NdArray, NodeId, OutOfRange, PixelRange, ScalarImage, Scale, Series,
    Text, Value, View3d,
};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{AxesHitKind, AxisMap, HitMap, LegendHit, LegendOwner};

const PLOT: Rect = Rect::new(50.0, 20.0, 200.0, 100.0);

fn single_2d() -> (FigureState, HitMap) {
    let state = FigureState::new(figure_with(vec![axes_2d(2)], vec![]));
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![],
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
    };
    (state, hit)
}

fn single_3d() -> (FigureState, HitMap) {
    let state = FigureState::new(figure_with(vec![axes_3d(2)], vec![]));
    let hit = HitMap {
        axes: vec![hit_3d(2, PLOT)],
        legend_entries: vec![],
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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

// Why: Refit undoes navigation, not content choices; a user who hid a series should not see it reappear because
// they reset the zoom. The changed flag drives recompilation, so a no-op reset must report no change.
#[test]
fn reset_view_restores_limits_and_views_but_keeps_visibility() {
    let mut axes = axes_2d(2);
    axes.artists.push(Artist::Series(Series {
        id: NodeId(10),
        ..Series::default()
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
    axes.artists.push(Artist::Series(Series {
        id: NodeId(10),
        label: Some(Text::new("series")),
        ..Series::default()
    }));
    let state = FigureState::new(figure_with(vec![axes], vec![]));
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![LegendHit {
            legend: LegendOwner::Axes(NodeId(2)),
            targets: vec![(NodeId(2), NodeId(10))],
            rect: Rect::new(200.0, 25.0, 45.0, 10.0),
        }],
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
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

/// Two axes, each with a line named "shared", and a hit map in which one entry of a legend panel stands for both.
fn figure_with_a_merged_panel_entry() -> (FigureState, HitMap) {
    let mut left = axes_2d(2);
    left.artists.push(Artist::Series(Series {
        id: NodeId(10),
        label: Some(Text::new("shared")),
        ..Series::default()
    }));
    let mut right = axes_2d(3);
    right.artists.push(Artist::Series(Series {
        id: NodeId(11),
        label: Some(Text::new("shared")),
        ..Series::default()
    }));
    let state = FigureState::new(figure_with(vec![left, right], vec![]));
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![LegendHit {
            legend: LegendOwner::Panel(NodeId(50)),
            targets: vec![(NodeId(2), NodeId(10)), (NodeId(3), NodeId(11))],
            rect: Rect::new(60.0, 200.0, 45.0, 10.0),
        }],
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars: vec![],
    };
    (state, hit)
}

// Why: a merged entry in a legend panel stands for one series drawn in several subplots, so a click on it must
// hide or show the series everywhere at once; and because the entry shows as visible while any of its artists is,
// a click when only some are hidden hides the rest, so the series is never left half shown by the click.
#[test]
fn clicking_a_merged_panel_entry_toggles_every_artist_it_stands_for() {
    let (mut state, hit) = figure_with_a_merged_panel_entry();
    let entry = Point::new(80.0, 205.0);

    assert!(state.click(&hit, entry));
    assert!(!visible(&state, 10) && !visible(&state, 11), "both hidden");
    assert!(state.click(&hit, entry));
    assert!(visible(&state, 10) && visible(&state, 11), "both shown");

    // Hide one artist through its own property, as the property editor would.
    state.record(&ironlab_ir::Transaction {
        edits: vec![ironlab_ir::Edit::Set {
            node: NodeId(10),
            path: ironlab_ir::PropertyPath::new(["visible"]).expect("a valid path"),
            value: Value::Bool(false),
        }],
    });
    assert!(
        state.click(&hit, entry),
        "the click hides the artist still shown"
    );
    assert!(!visible(&state, 10) && !visible(&state, 11), "both hidden");
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

/// Two stacked axes that each draw a line named "base", and a legend panel beneath them that both name, listing an
/// item for the base of the top axes and, below it, an item that stands for no artist. Returns the state and the
/// compiled hit map.
fn figure_with_an_explicit_key() -> (FigureState, HitMap) {
    use ironlab_ir::{
        Cell, Legend, LegendEntries, LegendEntryColumns, LegendEntryLayout, LegendItem,
        LegendSample, Panel, PanelContent, PanelLegend, PanelLocation, TileLayout,
    };
    let line = |id: u64| {
        Artist::Series(Series {
            id: NodeId(id),
            label: Some(Text::plain("base")),
            x: array_id(0),
            y: array_id(0),
            ..Series::default()
        })
    };
    let mut top = axes_2d(2);
    top.artists.push(line(10));
    top.legend = Some(Legend::Panel(NodeId(50)));
    let mut bottom = axes_2d(3);
    bottom.cell = Cell {
        row: 1,
        ..Cell::default()
    };
    bottom.artists.push(line(11));
    bottom.legend = Some(Legend::Panel(NodeId(50)));
    let mut figure = figure_with(vec![top, bottom], vec![]);
    figure.layout = TileLayout::new(3, 1);
    figure
        .data
        .push(data_array(array_id(0), NdArray::vector(vec![0.0, 1.0])));
    figure.panels.push(Panel {
        id: NodeId(50),
        cell: Cell {
            row: 2,
            ..Cell::default()
        },
        align: PanelLocation::Center,
        content: PanelContent::Legend(PanelLegend {
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Count(1),
                ..LegendEntryLayout::default()
            },
            entries: LegendEntries::Explicit(vec![
                LegendItem::artist(NodeId(10)),
                LegendItem::custom(LegendSample::swatch(Color::rgb(0.9, 0.6, 0.0)), "upper"),
            ]),
            ..PanelLegend::default()
        }),
    });
    let state = FigureState::new(figure);
    let hit = ironlab_scene::compile(state.figure(), &TEXT).hit_map;
    (state, hit)
}

// Why: an explicit key lists the base case once, by naming one of its artists; a click on that item must hide or
// show the base case in every axes that names the panel, as an automatic entry of the same name and look would. An
// item that stands for no artist is not a toggle, so a click on it changes nothing.
#[test]
fn clicking_an_explicit_item_toggles_its_series_and_a_custom_item_does_nothing() {
    let (mut state, hit) = figure_with_an_explicit_key();
    assert_eq!(
        hit.legend_entries.len(),
        1,
        "only the artist item is a click target"
    );
    let entry = hit.legend_entries[0].rect;
    let on_item = Point::new(entry.x + entry.width / 2.0, entry.y + entry.height / 2.0);
    let on_custom = Point::new(on_item.x, on_item.y + entry.height);

    assert!(
        !state.click(&hit, on_custom),
        "the custom item is not a toggle"
    );
    assert!(visible(&state, 10) && visible(&state, 11));

    assert!(state.click(&hit, on_item));
    assert!(!visible(&state, 10) && !visible(&state, 11), "both hidden");
    assert!(state.click(&hit, on_item));
    assert!(visible(&state, 10) && visible(&state, 11), "both shown");
}

// Why: a colour scale is not an artist, so a legend that lists one (#24) has nothing to hide or show for it: a click
// anywhere on a bar item or on the entry of a category that stands for no artist changes nothing, while the artist
// item beside them still toggles its series.
#[test]
fn clicking_a_bar_item_or_a_category_entry_does_nothing() {
    use ironlab_ir::{
        CategoricalColorScale, ColorCategory, ColorScale, ColorScaleOutOfRange,
        ContinuousColorScale, LegendEntries, LegendItem, PanelContent,
    };
    use ironlab_scene::hit::ColorbarOwner;
    let (state, _) = figure_with_an_explicit_key();
    let mut figure = state.figure().clone();
    let outside = ColorScaleOutOfRange {
        policy: OutOfRange::Strict,
        show_key: false,
        label: None,
    };
    figure.color_scales.push(ColorScale {
        id: NodeId(60),
        title: Some(Text::plain("Speed")),
        kind: ColorScaleKind::Continuous(ContinuousColorScale::default()),
        non_finite: outside.clone(),
    });
    figure.color_scales.push(ColorScale {
        id: NodeId(61),
        title: None,
        kind: ColorScaleKind::Categorical(CategoricalColorScale {
            categories: vec![ColorCategory {
                color: Color::rgb(0.0, 0.6, 0.3),
                label: Text::plain("taken"),
            }],
            outside: outside.clone(),
        }),
        non_finite: outside,
    });
    // The row of the panel fits its content, so that the bar item has the five rows it runs along.
    figure.layout.row_weights = vec![
        ironlab_ir::TrackSize::Weight(1.0),
        ironlab_ir::TrackSize::Weight(1.0),
        ironlab_ir::TrackSize::Fit,
    ];
    let PanelContent::Legend(legend) = &mut figure.panels[0].content else {
        panic!("the panel holds a legend");
    };
    legend.entries = LegendEntries::Explicit(vec![
        LegendItem::artist(NodeId(10)),
        LegendItem::color_scale(NodeId(60)),
        LegendItem::color_scale(NodeId(61)),
    ]);
    let mut state = FigureState::new(figure);
    let hit = ironlab_scene::compile(state.figure(), &TEXT).hit_map;
    assert_eq!(
        hit.legend_entries.len(),
        1,
        "only the artist item is a click target"
    );
    let bar = hit
        .colorbars
        .iter()
        .find(|c| c.owner == ColorbarOwner::Legend(LegendOwner::Panel(NodeId(50))))
        .expect("the legend draws a bar item")
        .bar;
    let panel = hit.panels[0].rect;
    let entry = hit.legend_entries[0].rect;
    // Every point of the panel outside the artist item: the bar item, its labels, and the category entry below it.
    for i in 0..=20 {
        for j in 0..=20 {
            let p = Point::new(
                panel.x + panel.width * f64::from(i) / 20.0,
                panel.y + panel.height * f64::from(j) / 20.0,
            );
            if entry.contains(p) {
                continue;
            }
            assert!(!state.click(&hit, p), "a click at {p:?} changes nothing");
        }
    }
    let on_bar = Point::new(bar.x + bar.width / 2.0, bar.y + bar.height / 2.0);
    assert!(!state.click(&hit, on_bar));
    assert!(visible(&state, 10) && visible(&state, 11));
    let on_item = Point::new(entry.x + entry.width / 2.0, entry.y + entry.height / 2.0);
    assert!(state.click(&hit, on_item), "the artist item still toggles");
    assert!(!visible(&state, 10));
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

// Why: Refit and double-click throw away every view change at once, which is exactly the action a user is most
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
    let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let y: Vec<f64> = (0..n)
        .map(|i| (i as f64 / 250.0).sin() + i as f64 / 1e6)
        .collect();
    let (xi, yi) = (array_id(0), array_id(1));
    let figure = Figure {
        id: NodeId(1),
        data: data_arrays([
            (xi, NdArray::vector(x.clone())),
            (yi, NdArray::vector(y.clone())),
        ]),
        axes: vec![Axes {
            id: NodeId(2),
            artists: vec![Artist::Series(Series {
                id: NodeId(3),
                label: Some(Text::plain("Measured")),
                x: xi,
                y: yi,
                ..Series::default()
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
    assert_eq!(tip.label.as_deref(), Some("Measured"));
    assert_ne!(
        tip.index, position_in_the_drawn_series,
        "the index reported is the one in the source data, not the place in the drawn series"
    );
    assert!(tip.index < n);
    assert_eq!(tip.x, Some(x[tip.index]));
    assert_eq!(tip.y, Some(y[tip.index]));
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

// ---------------------------------------------------------------------------------------------------------------
// Pixel datatips
// ---------------------------------------------------------------------------------------------------------------
//
// These tests compile the figure too, because what they are about is the agreement between where the scene compiler
// put the pixels of an image and the pixel, the centre and the stored value that the viewer reports for a pointer
// position over it.

/// A figure of one two-dimensional axes (node 2) with automatic limits, holding `artists` over `data`, whose arrays
/// take the identifiers 0, 1, … in the order given.
fn figure_of(data: Vec<NdArray>, artists: Vec<Artist>) -> Figure {
    Figure {
        id: NodeId(1),
        data: data
            .into_iter()
            .enumerate()
            .map(|(k, array)| data_array(array_id(k as u64), array))
            .collect(),
        axes: vec![Axes {
            id: NodeId(2),
            artists,
            ..Axes::default()
        }],
        color_scales: image_scales(),
        ..Figure::new()
    }
}

/// The state of a freshly opened `figure` and the hit map of its compilation.
fn compiled(figure: Figure) -> (FigureState, HitMap) {
    let state = FigureState::new(figure);
    let hit = ironlab_scene::compile(state.figure(), &TEXT).hit_map;
    (state, hit)
}

/// A two-dimensional array of `ny` rows and `nx` columns whose values are 0, 1, …, ny · nx − 1 in row order, so that
/// a value names the row and column it was read from.
fn numbered(ny: usize, nx: usize) -> NdArray {
    NdArray::from_shape(vec![ny, nx], (0..ny * nx).map(|k| k as f64).collect())
        .expect("the shape matches the values")
}

/// The placement in the xy plane with the given `(first, last)` centres along the columns and the rows.
fn placed(columns: Option<(f64, f64)>, rows: Option<(f64, f64)>) -> ImagePlacement {
    let range = |r: Option<(f64, f64)>| r.map(|(first, last)| PixelRange { first, last });
    ImagePlacement {
        columns: range(columns),
        rows: range(rows),
        ..ImagePlacement::default()
    }
}

/// A scalar image (node `id`) of the array `values`, mapped through the continuous colour scale of the image
/// fixtures and placed by `placement`.
fn scalar_image(id: u64, values: u64, placement: ImagePlacement) -> Artist {
    Artist::ScalarImage(ScalarImage {
        id: NodeId(id),
        values: array_id(values),
        color_scale: Some(CONTINUOUS_SCALE),
        placement,
        ..ScalarImage::default()
    })
}

/// The scalar image of `figure`, for a test that changes one of its properties before opening it.
fn scalar_image_mut(figure: &mut Figure, id: u64) -> &mut ScalarImage {
    match figure.artist_mut(NodeId(id)) {
        Some(Artist::ScalarImage(image)) => image,
        other => panic!("node {id} is a scalar image, not {other:?}"),
    }
}

/// The figure position of the data point `(x, y)` in the first axes of `hit`, which must be two-dimensional.
fn at_data(hit: &HitMap, x: f64, y: f64) -> Point {
    match &hit.axes[0].kind {
        AxesHitKind::TwoD { x: xm, y: ym } => Point::new(xm.to_figure(x), ym.to_figure(y)),
        AxesHitKind::ThreeD => panic!("the axes is three-dimensional, so it has no data mapping"),
    }
}

/// The pixel datatip at the data point `(x, y)`, or a panic naming the point.
fn pixel_tip_at(state: &FigureState, hit: &HitMap, x: f64, y: f64) -> PixelDatatip {
    state
        .pixel_datatip(hit, at_data(hit, x, y))
        .unwrap_or_else(|| panic!("a pixel lies under the data point ({x}, {y})"))
}

// Why: a pixel datatip is how a reader gets a number out of a raster, and it is only worth reading if it names the
// pixel the compiler put under the pointer and reads that pixel's value from the user's own array. Without ranges
// the compiler centres column i on x = i and row j on y = j, so the pixel found at the figure position of (i, j)
// must be (j, i), its centre must be (i, j), and its value must be the one the array holds at row j, column i,
// whether the array stores it as a float or as a byte. The `position` of the datatip is the figure-space position
// of that centre, not of the pointer, so that the canvas can mark the pixel as it rings a point; and the datatip
// must say which image of which axes it read, by name when the image has one, so the callout can be acted on.
#[test]
fn a_pixel_datatip_under_a_pixel_centre_names_that_pixel_its_centre_and_its_stored_value() {
    let mut figure = figure_with_scalar_image(3, 4);
    scalar_image_mut(&mut figure, 3).label = Some(Text::plain("Heat"));
    let (state, hit) = compiled(figure);

    for (j, i) in [(0, 0), (2, 3), (1, 2)] {
        let at = at_data(&hit, i as f64, j as f64);
        let tip = state
            .pixel_datatip(&hit, at)
            .unwrap_or_else(|| panic!("a pixel lies under the centre of ({j}, {i})"));
        assert_eq!(
            (tip.row, tip.column),
            (j, i),
            "the pixel at the centre of ({j}, {i})"
        );
        assert_close(tip.x, i as f64, EPS, "the x of the centre of the pixel");
        assert_close(tip.y, j as f64, EPS, "the y of the centre of the pixel");
        assert_eq!(
            tip.value,
            PixelValue::Value((j * 4 + i) as f64),
            "the value stored at row {j}, column {i}"
        );
        let centre = at_data(&hit, tip.x, tip.y);
        assert_close(
            tip.position.x,
            centre.x,
            EPS,
            "the x of the position is that of the pixel's centre",
        );
        assert_close(
            tip.position.y,
            centre.y,
            EPS,
            "the y of the position is that of the pixel's centre",
        );
    }
    let tip = pixel_tip_at(&state, &hit, 3.0, 2.0);
    assert_eq!(tip.axes, NodeId(2));
    assert_eq!(tip.artist, NodeId(3));
    assert_eq!(tip.label.as_deref(), Some("Heat"));

    let bytes =
        NdArray::from_shape_u8(vec![1, 2], vec![0, 255]).expect("the shape matches the values");
    let (state, hit) = compiled(figure_of(
        vec![bytes],
        vec![scalar_image(3, 0, ImagePlacement::default())],
    ));
    assert_eq!(
        pixel_tip_at(&state, &hit, 0.0, 0.0).value,
        PixelValue::Value(0.0),
        "a value stored as a byte reads as the byte"
    );
    assert_eq!(
        pixel_tip_at(&state, &hit, 1.0, 0.0).value,
        PixelValue::Value(255.0),
        "a value stored as a byte reads as the byte, 0 to 255"
    );
}

// Why: explicit ranges register an image with physical coordinates, and the coordinates the datatip shows must be
// those of the pixel's centre from the placement — `first + i · pitch` — rather than wherever the pointer happens to
// be inside the pixel, so that every position within one pixel reads as the one measurement that pixel holds; and
// the `position` is that centre in figure space, for the same reason.
#[test]
fn a_pixel_datatip_reports_the_pixel_centre_from_its_ranges_not_the_pointer() {
    // Four columns centred on 10, 20, 30 and 40 (pitch 10) and three rows centred on 1, 2 and 3 (pitch 1).
    let (state, hit) = compiled(figure_of(
        vec![numbered(3, 4)],
        vec![scalar_image(
            3,
            0,
            placed(Some((10.0, 40.0)), Some((1.0, 3.0))),
        )],
    ));

    let centred = pixel_tip_at(&state, &hit, 20.0, 2.0);
    assert_eq!((centred.row, centred.column), (1, 1));
    assert_close(centred.x, 20.0, EPS, "the x of the second centre");
    assert_close(centred.y, 2.0, EPS, "the y of the second centre");
    assert_eq!(centred.value, PixelValue::Value(5.0));

    // Pixel (1, 1) covers x from 15 to 25 and y from 1.5 to 2.5.
    let off_centre = pixel_tip_at(&state, &hit, 24.0, 1.6);
    assert_eq!((off_centre.row, off_centre.column), (1, 1));
    assert_close(off_centre.x, 20.0, EPS, "the centre, not the pointer's x");
    assert_close(off_centre.y, 2.0, EPS, "the centre, not the pointer's y");
    let centre = at_data(&hit, 20.0, 2.0);
    assert_close(
        off_centre.position.x,
        centre.x,
        EPS,
        "the position is the centre's, not the pointer's x",
    );
    assert_close(
        off_centre.position.y,
        centre.y,
        EPS,
        "the position is the centre's, not the pointer's y",
    );

    let last = pixel_tip_at(&state, &hit, 40.0, 3.0);
    assert_eq!((last.row, last.column), (2, 3));
    assert_close(last.x, 40.0, EPS, "the last centre lands on `last`");
    assert_close(last.y, 3.0, EPS, "the last centre lands on `last`");
    assert_eq!(last.value, PixelValue::Value(11.0));
}

// Why: a range running backwards mirrors the image by moving the pixels, not the samples, so row 0 stays at
// `rows.first` and the pitch is negative; a datatip that assumed increasing centres would name the mirror image of
// the pixel under the pointer and give it a coordinate on the wrong side of the image.
#[test]
fn a_pixel_datatip_follows_a_mirrored_range_so_that_row_zero_stays_at_the_first_centre() {
    // Three columns centred on 5, 3 and 1 (pitch −2) and four rows centred on 3, 2, 1 and 0 (pitch −1).
    let (state, hit) = compiled(figure_of(
        vec![numbered(4, 3)],
        vec![scalar_image(
            3,
            0,
            placed(Some((5.0, 1.0)), Some((3.0, 0.0))),
        )],
    ));

    let first = pixel_tip_at(&state, &hit, 5.0, 3.0);
    assert_eq!(
        (first.row, first.column),
        (0, 0),
        "row 0, column 0 lies at the first centres, which are the high ends of the ranges"
    );
    assert_close(first.x, 5.0, EPS, "x of the first centre");
    assert_close(first.y, 3.0, EPS, "y of the first centre");
    assert_eq!(first.value, PixelValue::Value(0.0));

    let tip = pixel_tip_at(&state, &hit, 1.0, 2.0);
    assert_eq!(
        (tip.row, tip.column),
        (1, 2),
        "x = 1 is the last column and y = 2 the second row"
    );
    assert_close(tip.x, 1.0, EPS, "x of the last column's centre");
    assert_close(tip.y, 2.0, EPS, "y of the second row's centre");
    assert_eq!(
        tip.value,
        PixelValue::Value(5.0),
        "row 1, column 2 of a 4 × 3 array"
    );
}

// Why: one pixel along an axis has no pitch to derive from two centres: the compiler makes it one data unit wide
// about its first centre and ignores `last`, and the datatip must agree, or the coordinate shown for a one-column
// image would depend on a `last` that placed nothing.
#[test]
fn a_pixel_datatip_on_a_single_column_reports_the_first_centre_and_ignores_last() {
    // One column centred on 2 (its `last` of 7 is ignored) and three rows centred on 0, 1 and 2.
    let (state, hit) = compiled(figure_of(
        vec![numbered(3, 1)],
        vec![scalar_image(3, 0, placed(Some((2.0, 7.0)), None))],
    ));

    // The column covers x from 1.5 to 2.5.
    let tip = pixel_tip_at(&state, &hit, 2.4, 1.0);
    assert_eq!((tip.row, tip.column), (1, 0));
    assert_close(tip.x, 2.0, EPS, "the centre of the one column");
    assert_close(tip.y, 1.0, EPS, "the centre of the second row");
    assert_eq!(tip.value, PixelValue::Value(1.0));
    assert!(
        state.pixel_datatip(&hit, at_data(&hit, 7.0, 1.0)).is_none(),
        "nothing lies at the ignored `last`"
    );
}

// Why: a transparent pixel is one the artist chose not to colour, not one without a value: a NaN in a field and a
// value outside the colour limits are exactly what a reader hovers over to find out what is there, so the datatip
// must report the stored value however the pixel was painted.
#[test]
fn a_transparent_pixel_still_reports_its_stored_value_including_nan() {
    let values = NdArray::from_shape(vec![1, 3], vec![f64::NAN, 0.0, 2.5])
        .expect("the shape matches the values");
    let mut figure = figure_of(
        vec![values],
        vec![scalar_image(3, 0, ImagePlacement::default())],
    );
    continuous_scale_mut(&mut figure).limits = manual(2.0, 3.0);
    let state = FigureState::new(figure);
    let scene = ironlab_scene::compile(state.figure(), &TEXT);
    let item = find_image(&scene.display_list.items).expect("the image is drawn");
    assert_eq!(
        image_sample(item, 0, 0)[3],
        0,
        "precondition: the NaN pixel is transparent"
    );
    assert_eq!(
        image_sample(item, 0, 1)[3],
        0,
        "precondition: the pixel below the colour limits is transparent"
    );

    let nan = pixel_tip_at(&state, &scene.hit_map, 0.0, 0.0);
    assert!(
        matches!(nan.value, PixelValue::Value(v) if v.is_nan()),
        "the NaN is reported as stored: {:?}",
        nan.value
    );
    assert_eq!(
        pixel_tip_at(&state, &scene.hit_map, 1.0, 0.0).value,
        PixelValue::Value(0.0),
        "a value below the limits is reported as stored"
    );
    assert_eq!(
        pixel_tip_at(&state, &scene.hit_map, 2.0, 0.0).value,
        PixelValue::Value(2.5),
        "a value inside the limits is reported as stored"
    );
}

// Why: a scalar image through an indexed colour scale looks its entries up by the index as stored, and the datatip
// shows the reader that index: an 8-bit index as the number it is, and a floating-point index as stored rather than
// truncated, because 2.9 names no entry while 2 does, and it is the data the reader asked about.
#[test]
fn a_pixel_datatip_on_an_indexed_image_reports_the_stored_index_untruncated() {
    let indexed = |id: u64, indices: u64| {
        Artist::ScalarImage(ScalarImage {
            id: NodeId(id),
            values: array_id(indices),
            color_scale: Some(PALETTE_SCALE),
            ..ScalarImage::default()
        })
    };

    let bytes =
        NdArray::from_shape_u8(vec![1, 2], vec![7, 255]).expect("the shape matches the values");
    let (state, hit) = compiled(figure_of(vec![bytes], vec![indexed(3, 0)]));
    let tip = pixel_tip_at(&state, &hit, 0.0, 0.0);
    assert_eq!(
        tip.value,
        PixelValue::Index(7.0),
        "an 8-bit index is the number it denotes"
    );
    assert_eq!(
        pixel_tip_at(&state, &hit, 1.0, 0.0).value,
        PixelValue::Index(255.0)
    );
    assert_eq!(tip.artist, NodeId(3));
    assert_eq!(tip.axes, NodeId(2));
    assert_eq!(
        tip.label, None,
        "an image without a display name has none to show"
    );

    let floats =
        NdArray::from_shape(vec![1, 2], vec![2.9, -1.5]).expect("the shape matches the values");
    let (state, hit) = compiled(figure_of(vec![floats], vec![indexed(3, 0)]));
    assert_eq!(
        pixel_tip_at(&state, &hit, 0.0, 0.0).value,
        PixelValue::Index(2.9),
        "a floating-point index is reported as stored, not truncated to 2"
    );
    assert_eq!(
        pixel_tip_at(&state, &hit, 1.0, 0.0).value,
        PixelValue::Index(-1.5),
        "an index that names no entry, drawn transparent, is still reported as stored"
    );
}

// Why: a scalar image through a categorical colour scale is a map of classes, and the reader hovers a pixel to learn
// which class it holds, so the datatip carries the label of the category the stored index names, beside the index
// itself; an index that names no category (a fraction, or one past the last) has no label to carry, and the
// datatip reports the index alone rather than the label of a neighbouring category.
#[test]
fn a_pixel_datatip_through_a_categorical_scale_names_the_category_of_the_index() {
    use ironlab_ir::{CategoricalColorScale, ColorCategory, ColorScale, ColorScaleOutOfRange};

    const CLASSES: NodeId = NodeId(92);
    let indices =
        NdArray::from_shape(vec![1, 3], vec![1.0, 0.5, 2.0]).expect("the shape matches the values");
    let mut figure = figure_of(
        vec![indices],
        vec![Artist::ScalarImage(ScalarImage {
            id: NodeId(3),
            values: array_id(0),
            color_scale: Some(CLASSES),
            ..ScalarImage::default()
        })],
    );
    figure.color_scales.push(ColorScale {
        id: CLASSES,
        kind: ColorScaleKind::Categorical(CategoricalColorScale {
            categories: ["Water", "Forest"]
                .into_iter()
                .map(|label| ColorCategory {
                    color: Color::rgb(0.0, 0.5, 1.0),
                    label: Text::plain(label),
                })
                .collect(),
            outside: ColorScaleOutOfRange::new(OutOfRange::Transparent),
        }),
        ..ColorScale::default()
    });
    let (state, hit) = compiled(figure);
    assert_eq!(
        pixel_tip_at(&state, &hit, 0.0, 0.0).value,
        PixelValue::Category {
            index: 1.0,
            label: Some("Forest".to_owned()),
        },
        "index 1 names the second category"
    );
    for (x, index) in [(1.0, 0.5), (2.0, 2.0)] {
        assert_eq!(
            pixel_tip_at(&state, &hit, x, 0.0).value,
            PixelValue::Category { index, label: None },
            "the index {index} names no category"
        );
    }
}

// Why: a true-colour pixel has no single number, so the datatip shows its components, in the units the array
// stores them — bytes from 0 to 255, or fractions from 0 to 1 — with an alpha component when the array has one, so
// that what the reader sees is what their array holds and not a conversion they would have to undo.
#[test]
fn a_pixel_datatip_on_a_true_colour_image_reports_the_components_in_stored_units() {
    let image = |id: u64, pixels: u64| {
        Artist::Image(Image {
            id: NodeId(id),
            pixels: array_id(pixels),
            ..Image::default()
        })
    };

    let bytes = NdArray::from_shape_u8(vec![1, 2, 3], vec![255, 0, 128, 1, 2, 3])
        .expect("the shape matches the values");
    let (state, hit) = compiled(figure_of(vec![bytes], vec![image(3, 0)]));
    assert_eq!(
        pixel_tip_at(&state, &hit, 0.0, 0.0).value,
        PixelValue::Components(vec![255.0, 0.0, 128.0]),
        "three 8-bit components, as bytes"
    );
    assert_eq!(
        pixel_tip_at(&state, &hit, 1.0, 0.0).value,
        PixelValue::Components(vec![1.0, 2.0, 3.0]),
        "the components of the second column"
    );

    let floats = NdArray::from_shape(
        vec![2, 1, 4],
        vec![0.5, 0.25, 1.0, 0.75, 0.0, 0.0, 0.0, 0.0],
    )
    .expect("the shape matches the values");
    let (state, hit) = compiled(figure_of(vec![floats], vec![image(3, 0)]));
    assert_eq!(
        pixel_tip_at(&state, &hit, 0.0, 0.0).value,
        PixelValue::Components(vec![0.5, 0.25, 1.0, 0.75]),
        "four floating-point components with alpha, as fractions"
    );
    assert_eq!(
        pixel_tip_at(&state, &hit, 0.0, 1.0).value,
        PixelValue::Components(vec![0.0; 4]),
        "a fully transparent pixel still reports its components"
    );
}

// Why: the viewer answers for a pixel only where the compiler drew one, and the hit map records an image only when
// it was drawn in a two-dimensional axes: a hidden image and one on the floor of a three-dimensional axes (whose
// placement has no inverse until three-dimensional datatips exist) leave nothing to read, as does the space beside
// an image. A datatip invented there would report data the reader cannot see.
#[test]
fn a_pixel_datatip_is_read_only_where_a_drawn_image_lies_in_a_two_dimensional_axes() {
    let image = || {
        figure_of(
            vec![numbered(2, 2)],
            vec![scalar_image(3, 0, ImagePlacement::default())],
        )
    };

    // Limits wider than the image, so that there is room inside the axes beside it.
    let mut beside = image();
    let axes = beside.axes_mut(NodeId(2)).expect("axes exists");
    axes.x.limits = manual(-2.0, 6.0);
    axes.y.limits = manual(-2.0, 4.0);
    let (state, hit) = compiled(beside);
    assert!(
        state.pixel_datatip(&hit, at_data(&hit, 1.0, 1.0)).is_some(),
        "precondition: the image is read where it lies"
    );
    assert!(
        state.pixel_datatip(&hit, at_data(&hit, 4.0, 1.0)).is_none(),
        "inside the axes but beside the image"
    );
    assert!(
        state.pixel_datatip(&hit, at_data(&hit, 1.0, 3.0)).is_none(),
        "inside the axes but above the image"
    );
    assert!(
        state.pixel_datatip(&hit, Point::new(2.0, 2.0)).is_none(),
        "the corner of the page, outside every axes"
    );

    let mut hidden = image();
    scalar_image_mut(&mut hidden, 3).visible = false;
    let (state, hit) = compiled(hidden);
    assert!(
        state.pixel_datatip(&hit, at_data(&hit, 1.0, 1.0)).is_none(),
        "a hidden image"
    );

    let solid = Figure {
        id: NodeId(1),
        data: data_arrays([(array_id(0), numbered(2, 2))]),
        color_scales: image_scales(),
        axes: vec![Axes {
            artists: vec![scalar_image(3, 0, ImagePlacement::default())],
            ..axes_3d(2)
        }],
        ..Figure::new()
    };
    let (state, hit) = compiled(solid);
    let plot = hit.axes[0].plot_rect;
    assert!(
        state
            .pixel_datatip(
                &hit,
                Point::new(plot.x + plot.width / 2.0, plot.y + plot.height / 2.0)
            )
            .is_none(),
        "an image on the floor of a three-dimensional axes"
    );
}

// Why: where images overlap the reader sees the one painted last, so that is the one whose pixel must be reported;
// reporting the first would describe a pixel hidden beneath another, and where only the first lies there is nothing
// else to report.
#[test]
fn where_two_images_overlap_the_pixel_of_the_one_painted_last_is_reported() {
    // The first image covers x from −0.5 to 1.5 and the second, painted over it, x from 0.5 to 2.5.
    let (state, hit) = compiled(figure_of(
        vec![numbered(1, 2), numbered(1, 2)],
        vec![
            scalar_image(3, 0, ImagePlacement::default()),
            scalar_image(4, 1, placed(Some((1.0, 2.0)), None)),
        ],
    ));

    let overlapped = pixel_tip_at(&state, &hit, 1.0, 0.0);
    assert_eq!(overlapped.artist, NodeId(4), "the image painted last");
    assert_eq!(
        (overlapped.row, overlapped.column),
        (0, 0),
        "its own first column, not the first image's second"
    );
    assert_close(
        overlapped.x,
        1.0,
        EPS,
        "the centre of the first column of the second image",
    );
    assert_eq!(overlapped.value, PixelValue::Value(0.0));

    let alone = pixel_tip_at(&state, &hit, 0.0, 0.0);
    assert_eq!(alone.artist, NodeId(3), "where only the first image lies");
    assert_eq!((alone.row, alone.column), (0, 0));
    assert_eq!(alone.value, PixelValue::Value(0.0));
}

// Why: a line drawn over an image is small and painted on top, so a reader pointing at one of its points means the
// point, and the callout must name it rather than the pixel beneath; away from any point the pixel is what is there,
// and where neither is, nothing is.
#[test]
fn tip_at_prefers_a_drawn_point_within_reach_over_the_pixel_beneath_it() {
    // A 4 × 4 image at default placement with a line through the centres of its diagonal pixels drawn over it.
    let diagonal = NdArray::vector(vec![0.0, 1.0, 2.0, 3.0]);
    let (state, hit) = compiled(figure_of(
        vec![numbered(4, 4), diagonal.clone(), diagonal],
        vec![
            scalar_image(3, 0, ImagePlacement::default()),
            Artist::Series(Series {
                id: NodeId(4),
                x: array_id(1),
                y: array_id(2),
                ..Series::default()
            }),
        ],
    ));
    let drawn = &hit.artists[0];
    assert_eq!(drawn.artist, NodeId(4), "precondition: the line was drawn");
    assert_eq!(hit.images.len(), 1, "precondition: the image was drawn");

    let on_point = drawn.samples[1].position;
    match state.tip_at(&hit, on_point) {
        Some(Tip::Point(tip)) => {
            assert_eq!(tip.artist, NodeId(4));
            assert_eq!(tip.index, 1);
        }
        other => panic!("a point is preferred over the pixel beneath it, not {other:?}"),
    }
    let within_reach = Point::new(on_point.x + DATATIP_RADIUS_POINTS - 0.5, on_point.y);
    assert!(
        matches!(state.tip_at(&hit, within_reach), Some(Tip::Point(_))),
        "the point is preferred as far as it can be read"
    );

    // The centre of pixel (0, 3) is three data units from the nearest points of the line, at (0, 0) and (3, 3).
    let away = at_data(&hit, 3.0, 0.0);
    assert!(
        hit.sample_at(away, DATATIP_RADIUS_POINTS).is_none(),
        "precondition: no point within reach"
    );
    match state.tip_at(&hit, away) {
        Some(Tip::Pixel(pixel)) => {
            assert_eq!(pixel.artist, NodeId(3));
            assert_eq!((pixel.row, pixel.column), (0, 3));
            assert_eq!(pixel.value, PixelValue::Value(3.0));
        }
        other => panic!("the pixel is reported where no point is near, not {other:?}"),
    }

    assert!(
        state.tip_at(&hit, Point::new(2.0, 2.0)).is_none(),
        "nothing is reported where neither a point nor a pixel lies"
    );
}

// Why: the property editor changes an out-of-range policy of a colour scale through the same overlay as every other
// edit, and the point of a policy is what the canvas then shows: setting `below` from transparent to a colour must
// make the recompiled image opaque in that colour where it was see-through and change nothing else, while the
// source figure keeps its own policy so that the change can be reverted.
#[test]
fn setting_the_below_policy_to_a_colour_recolours_the_transparent_pixels_of_the_recompiled_image() {
    // Values 0 and 1 lie below colour limits of [2, 3]; 2 and 3 lie at them.
    let mut figure = figure_of(
        vec![numbered(2, 2)],
        vec![scalar_image(3, 0, ImagePlacement::default())],
    );
    continuous_scale_mut(&mut figure).limits = manual(2.0, 3.0);
    let mut state = FigureState::new(figure);
    let samples = |state: &FigureState| {
        let scene = ironlab_scene::compile(state.figure(), &TEXT);
        let item = find_image(&scene.display_list.items).expect("the image is drawn");
        [
            image_sample(item, 0, 0),
            image_sample(item, 0, 1),
            image_sample(item, 1, 0),
            image_sample(item, 1, 1),
        ]
    };
    let before = samples(&state);
    assert_eq!(
        before[0][3], 0,
        "precondition: 0 is below the limits and transparent"
    );
    assert_eq!(
        before[1][3], 0,
        "precondition: 1 is below the limits and transparent"
    );
    assert_eq!(
        before[2][3], 255,
        "precondition: 2 is at the lower limit and opaque"
    );
    assert_eq!(
        before[3][3], 255,
        "precondition: 3 is at the upper limit and opaque"
    );

    assert!(
        state.try_record(&set(
            CONTINUOUS_SCALE.0,
            "kind.continuous.below.policy",
            Value::OutOfRange(OutOfRange::Rgba {
                color: Color::rgb(1.0, 0.0, 0.0)
            })
        )),
        "the edit is accepted: {:?}",
        state.problems()
    );

    let after = samples(&state);
    assert_eq!(
        after[0],
        [255, 0, 0, 255],
        "the pixel below the limits takes the colour"
    );
    assert_eq!(after[1], [255, 0, 0, 255], "and so does the other");
    assert_eq!(
        after[2], before[2],
        "a pixel inside the limits keeps its colour"
    );
    assert_eq!(
        after[3], before[3],
        "a pixel inside the limits keeps its colour"
    );
    assert!(state.problems().is_empty(), "{:?}", state.problems());
    match state
        .source()
        .color_scale(CONTINUOUS_SCALE)
        .map(|scale| &scale.kind)
    {
        Some(ColorScaleKind::Continuous(scale)) => assert_eq!(
            scale.below.policy,
            OutOfRange::Transparent,
            "the source keeps its own policy"
        ),
        other => panic!("the source holds its continuous scale, not {other:?}"),
    }
}

// Why: an image cannot be placed on a logarithmic axis, so switching the scale of an axis beneath one in the property
// editor would leave the figure invalid. The edit must be refused like any other edit that validation rejects: the
// displayed figure keeps its linear axis, and the problems indicator gives the reason, which names the property and
// tells the user to draw the values as a surface, so that the user learns what to do instead.
#[test]
fn switching_the_axis_beneath_an_image_to_a_logarithmic_scale_is_refused_with_the_reason() {
    let figure = figure_of(
        vec![numbered(2, 2)],
        vec![scalar_image(3, 0, ImagePlacement::default())],
    );
    let mut state = FigureState::new(figure);
    let before = state.figure().clone();

    let changed = state.try_record(&set(2, "x.scale", Value::Scale(Scale::Log)));

    assert!(!changed, "a refused edit changes nothing");
    assert_eq!(state.figure(), &before);
    assert_eq!(state.problems().len(), 1, "{:?}", state.problems());
    let problem = &state.problems()[0];
    assert_eq!(problem.origin, Origin::Refused);
    assert_eq!(
        problem.path.as_ref().map(ToString::to_string),
        Some("x.scale".to_owned()),
        "the problem names the property"
    );
    assert!(
        problem.detail.contains("surface"),
        "the reason says what to draw instead: {}",
        problem.detail
    );
}

// Why: a fraction of the plot area is where a mark was drawn, not a measurement, so a datatip that reported it as `y`
// would tell the reader that a tick along the top edge sits at y = 0.985 whatever the axis shows. The datatip of a
// blended artist reports its data coordinate only, both in its values and in the text of its callout.
#[test]
fn a_datatip_on_a_blended_artist_reports_its_data_coordinate_only() {
    let (alpha, top) = (array_id(0), array_id(1));
    let figure = Figure {
        id: NodeId(1),
        data: data_arrays([
            (alpha, NdArray::vector(vec![-2.0, 3.5])),
            (top, NdArray::vector(vec![0.985, 0.985])),
        ]),
        axes: vec![Axes {
            id: NodeId(2),
            x: ironlab_ir::Axis {
                limits: Limits::Manual {
                    min: -5.0,
                    max: 5.0,
                },
                ..ironlab_ir::Axis::default()
            },
            artists: vec![Artist::Series(Series {
                id: NodeId(3),
                x: alpha,
                y: top,
                y_coords: Coords::AxesFraction,
                clip: false,
                line: ironlab_ir::LineStyle {
                    dash: ironlab_ir::DashStyle::None,
                    ..ironlab_ir::LineStyle::default()
                },
                marker: ironlab_ir::MarkerStyle {
                    shape: ironlab_ir::MarkerShape::VLine,
                    ..ironlab_ir::MarkerStyle::default()
                },
                ..Series::default()
            })],
            ..Axes::default()
        }],
        ..Figure::new()
    };
    let (state, hit) = compiled(figure);
    let sample = hit.artists[0].samples[1];
    let tip = state
        .datatip(&hit, sample.position)
        .expect("a drawn tick lies under its own position");
    assert_eq!(tip.index, 1);
    assert_eq!(tip.x, Some(3.5));
    assert_eq!(tip.y, None, "the fraction is not reported as data");
    let text = ironlab_canvas::figure_canvas::datatip_text(&tip);
    assert!(text.contains("x = 3.5"), "{text}");
    assert!(!text.contains("y ="), "{text}");
}

/// A figure of one 2D axes holding the line `line`, whose x and y data are `x` and `y`.
fn figure_with_line(x: Vec<f64>, y: Vec<f64>, line: Series) -> Figure {
    let (xi, yi) = (array_id(0), array_id(1));
    Figure {
        id: NodeId(1),
        data: data_arrays([(xi, NdArray::vector(x)), (yi, NdArray::vector(y))]),
        axes: vec![Axes {
            id: NodeId(2),
            artists: vec![Artist::Series(Series {
                id: NodeId(3),
                x: xi,
                y: yi,
                ..line
            })],
            ..Axes::default()
        }],
        ..Figure::new()
    }
}

// Why: a series coloured by data shows a third quantity in its colours, which the reader cannot read back from the
// picture, so the datatip of a point reports the colour value the user supplied for it, at its own index, beside its
// coordinates; with one value per segment, the value of the segment that leaves the point; and a series without
// colour data reports none.
#[test]
fn a_datatip_on_a_series_coloured_by_data_reports_the_colour_value_of_its_point() {
    let coloured = |colours: Vec<f64>| {
        let mut figure = figure_with_line(
            vec![0.0, 1.0, 2.0],
            vec![0.0, 1.0, 0.0],
            Series {
                color_data: Some(array_id(2)),
                color_scale: Some(NodeId(90)),
                line: ironlab_ir::LineStyle {
                    color: ironlab_ir::ColorSpec::Colormapped,
                    ..ironlab_ir::LineStyle::default()
                },
                ..Series::default()
            },
        );
        figure.data.push(ironlab_ir::DataArray {
            id: array_id(2),
            name: None,
            array: NdArray::vector(colours),
        });
        figure.color_scales.push(ironlab_ir::ColorScale {
            id: NodeId(90),
            ..ironlab_ir::ColorScale::default()
        });
        figure
    };
    let tip_at = |figure: Figure, index: usize| {
        let (state, hit) = compiled(figure);
        let sample = hit.artists[0]
            .samples
            .iter()
            .find(|sample| sample.source_index == index)
            .copied()
            .expect("every point is drawn");
        state
            .datatip(&hit, sample.position)
            .expect("a drawn point lies under its own position")
    };

    let tip = tip_at(coloured(vec![10.0, 20.5, 30.0]), 1);
    assert_eq!(tip.index, 1);
    assert_eq!(tip.color, Some(20.5), "the value of the point itself");
    let text = ironlab_canvas::figure_canvas::datatip_text(&tip);
    assert!(text.contains("c = 20.5"), "{text}");

    let per_segment = coloured(vec![-1.0, 4.0]);
    assert_eq!(tip_at(per_segment.clone(), 1).color, Some(4.0));
    assert_eq!(
        tip_at(per_segment, 2).color,
        None,
        "no segment leaves the last point"
    );

    let plain = figure_with_line(vec![0.0, 1.0], vec![0.0, 1.0], Series::default());
    let tip = tip_at(plain, 0);
    assert_eq!(tip.color, None);
    assert!(
        !ironlab_canvas::figure_canvas::datatip_text(&tip).contains("c ="),
        "a series without colour data reports no colour value"
    );
}

// Why: a horizontal reference line is placed at a data y across fractions of the plot width, so its datatip reports
// the y it stands for and no x, and its callout says so.
#[test]
fn a_datatip_on_a_horizontal_reference_line_reports_its_y_only() {
    let figure = figure_with_line(
        vec![0.0, 1.0],
        vec![2.5, 2.5],
        Series {
            x_coords: Coords::AxesFraction,
            ..Series::default()
        },
    );
    let (state, hit) = compiled(figure);
    let sample = hit.artists[0].samples[0];
    let tip = state
        .datatip(&hit, sample.position)
        .expect("a drawn end lies under its own position");
    assert_eq!(tip.x, None);
    assert_eq!(tip.y, Some(2.5));
    let text = ironlab_canvas::figure_canvas::datatip_text(&tip);
    assert!(text.contains("y = 2.5") && !text.contains("x ="), "{text}");
}

// Why: a mark that is not clipped can be drawn above the plot area, where the reader can see it and point at it, so
// the datatip must find it there as it finds any drawn point.
#[test]
fn a_datatip_finds_an_unclipped_mark_drawn_beyond_the_plot_area() {
    let figure = figure_with_line(
        vec![0.25, 0.75],
        vec![1.05, 1.05],
        Series {
            y_coords: Coords::AxesFraction,
            clip: false,
            marker: ironlab_ir::MarkerStyle {
                shape: ironlab_ir::MarkerShape::VLine,
                ..ironlab_ir::MarkerStyle::default()
            },
            ..Series::default()
        },
    );
    let (state, hit) = compiled(figure);
    let plot = hit.axes[0].plot_rect;
    let sample = hit.artists[0].samples[1];
    assert!(
        sample.position.y < plot.y,
        "the mark lies above the plot area"
    );
    let tip = state
        .datatip(&hit, sample.position)
        .expect("the mark is found where it was drawn");
    assert_eq!((tip.index, tip.x, tip.y), (1, Some(0.75), None));
}

// ---------------------------------------------------------------------------------------------------------------
// Reversed axes: every gesture behaves on an axis drawn from its upper limit as it does on a normal one
// ---------------------------------------------------------------------------------------------------------------

// These tests compile the figure, because the direction of an axis is decided by the scene compiler and reaches the
// viewer only through the axis maps of the hit map.

/// A two-dimensional axes (node 2) with manual limits x ∈ [0, 10] (logarithmic from 1 to 100 when `log`) and
/// y ∈ [−2, 1], whose x axis is reversed when `reverse_x` and whose y axis is always reversed, holding one line
/// (node 3) through three points.
fn reversed_figure(reverse_x: bool, log: bool) -> Figure {
    let mut figure = figure_of(
        vec![
            NdArray::vector(vec![2.0, 5.0, 8.0]),
            NdArray::vector(vec![-1.5, 0.5, -0.5]),
        ],
        vec![Artist::Series(Series {
            id: NodeId(3),
            x: array_id(0),
            y: array_id(1),
            label: Some(Text::plain("Cp")),
            ..Series::default()
        })],
    );
    let axes = figure.axes_mut(NodeId(2)).expect("the axes exists");
    axes.x.limits = if log {
        manual(1.0, 100.0)
    } else {
        manual(0.0, 10.0)
    };
    axes.x.scale = if log { Scale::Log } else { Scale::Linear };
    axes.x.reverse = reverse_x;
    axes.y.limits = manual(-2.0, 1.0);
    axes.y.reverse = true;
    figure
}

/// Returns the x and y axis maps of axes 2 in a compiled hit map, checking that the y axis, and the x axis when
/// `reverse_x`, runs from its upper limit, so that a test of a gesture never passes on an axis that was not reversed.
#[track_caller]
fn reversed_maps_of(hit: &HitMap, reverse_x: bool) -> (AxisMap, AxisMap) {
    let (x, y) = maps_of(hit);
    assert_eq!(x.start > x.end, reverse_x, "the direction of x: {x:?}");
    assert!(
        y.start < y.end,
        "y runs from its upper limit at the bottom: {y:?}"
    );
    (x, y)
}

/// Returns the x and y axis maps of axes 2 in a compiled hit map.
fn maps_of(hit: &HitMap) -> (AxisMap, AxisMap) {
    match hit.axes[0].kind {
        AxesHitKind::TwoD { x, y } => (x, y),
        AxesHitKind::ThreeD => panic!("axes 2 is two-dimensional"),
    }
}

/// Compiles the displayed figure of `state` and returns its hit map.
fn recompiled(state: &FigureState) -> HitMap {
    ironlab_scene::compile(state.figure(), &TEXT).hit_map
}

// Why: a pan grabs the data under the pointer and keeps it there; on a reversed axis the data moves opposite to the
// limits, and a pan computed for a normal axis would push the view the wrong way, away from the pointer.
#[test]
fn pan_on_reversed_axes_keeps_the_grabbed_data_under_the_pointer() {
    for log in [false, true] {
        let (mut state, hit) = compiled(reversed_figure(true, log));
        let (x, y) = reversed_maps_of(&hit, true);
        let plot = hit.axes[0].plot_rect;
        let start = Point::new(plot.x + 0.3 * plot.width, plot.y + 0.4 * plot.height);
        let end = Point::new(start.x + 40.0, start.y + 25.0);
        let grabbed = (x.to_data(start.x), y.to_data(start.y));

        assert!(drag(&mut state, &hit, start, end));

        // The new limits are read through the figure-space extent of the plot as the gesture saw it, because the
        // compiler may move the plot when new tick labels change the width of the margins.
        let (xmin, xmax) = manual_of(state.figure(), 2, Dimension::X);
        let (ymin, ymax) = manual_of(state.figure(), 2, Dimension::Y);
        assert!(xmin < xmax && ymin < ymax, "the limits still increase");
        let (x, y) = (remapped(x, (xmin, xmax)), remapped(y, (ymin, ymax)));
        assert_close(
            x.to_figure(grabbed.0),
            end.x,
            1e-6,
            "grabbed x under pointer",
        );
        assert_close(
            y.to_figure(grabbed.1),
            end.y,
            1e-6,
            "grabbed y under pointer",
        );
        // Recompiling keeps the direction: the larger value is still at the left and at the bottom.
        let (x, y) = maps_of(&recompiled(&state));
        assert!(x.to_figure(xmax) < x.to_figure(xmin));
        assert!(y.to_figure(ymax) > y.to_figure(ymin));
    }
}

// Why: the wheel zooms about the cursor, and on a reversed axis the cursor's position must still name the same data
// value after the zoom, or each notch would slide the feature under inspection away.
#[test]
fn wheel_zoom_on_reversed_axes_keeps_the_data_under_the_cursor() {
    let (mut state, hit) = compiled(reversed_figure(true, false));
    let (x, y) = reversed_maps_of(&hit, true);
    let plot = hit.axes[0].plot_rect;
    let at = Point::new(plot.x + 0.7 * plot.width, plot.y + 0.2 * plot.height);
    let under = (x.to_data(at.x), y.to_data(at.y));

    assert!(state.scroll(&hit, at, 2.0));

    let (xmin, xmax) = manual_of(state.figure(), 2, Dimension::X);
    let (ymin, ymax) = manual_of(state.figure(), 2, Dimension::Y);
    let (x, y) = (remapped(x, (xmin, xmax)), remapped(y, (ymin, ymax)));
    assert_close(x.to_data(at.x), under.0, 1e-6, "x under the cursor");
    assert_close(y.to_data(at.y), under.1, 1e-6, "y under the cursor");
    assert_close(ymax - ymin, 1.5, 1e-9, "the y range halves");
}

// Why: a rubber band selects the data inside it; on a reversed axis the top of the band is the lower value, and the
// new limits must still be increasing and cover exactly the data the band covered.
#[test]
fn box_zoom_on_reversed_axes_sets_increasing_limits_to_the_data_in_the_band() {
    let (mut state, hit) = compiled(reversed_figure(true, false));
    state.tool = Tool::Zoom;
    let (x, y) = reversed_maps_of(&hit, true);
    let plot = hit.axes[0].plot_rect;
    let from = Point::new(plot.x + 0.2 * plot.width, plot.y + 0.1 * plot.height);
    let to = Point::new(plot.x + 0.6 * plot.width, plot.y + 0.7 * plot.height);
    let (x0, x1) = (x.to_data(from.x), x.to_data(to.x));
    let (y0, y1) = (y.to_data(from.y), y.to_data(to.y));

    assert!(drag(&mut state, &hit, from, to));

    let (xmin, xmax) = manual_of(state.figure(), 2, Dimension::X);
    let (ymin, ymax) = manual_of(state.figure(), 2, Dimension::Y);
    assert_close(xmin, x0.min(x1), 1e-9, "x min");
    assert_close(xmax, x0.max(x1), 1e-9, "x max");
    assert_close(ymin, y0.min(y1), 1e-9, "y min");
    assert_close(ymax, y0.max(y1), 1e-9, "y max");
    // The direction is a property of the figure, which zooming does not change.
    let axes = state.figure().axes(NodeId(2)).unwrap();
    assert!(axes.x.reverse && axes.y.reverse);
}

// Why: the datatip reports the quantity the reader reads off the axis, so on a reversed C_p axis it reports C_p as
// stored, not its negation, and finds the point where it is drawn.
#[test]
fn a_datatip_on_reversed_axes_reports_the_stored_values_of_the_point_under_the_pointer() {
    let (state, hit) = compiled(reversed_figure(true, false));
    let (x, y) = reversed_maps_of(&hit, true);
    let at = Point::new(x.to_figure(5.0), y.to_figure(0.5));
    let tip = state.datatip(&hit, at).expect("a point is drawn there");
    assert_eq!(tip.index, 1);
    assert_eq!((tip.x, tip.y), (Some(5.0), Some(0.5)));
}

// Why: double-clicking restores the limits the figure was opened with; the direction of the axes is not a view
// property and must be exactly what it was, while the limits return to their source values.
#[test]
fn double_click_on_reversed_axes_restores_the_limits_and_keeps_the_direction() {
    let (mut state, hit) = compiled(reversed_figure(false, false));
    reversed_maps_of(&hit, false);
    let plot = hit.axes[0].plot_rect;
    let at = Point::new(plot.x + 0.5 * plot.width, plot.y + 0.5 * plot.height);
    assert!(state.scroll(&hit, at, 3.0));
    assert!(state.double_click(&hit, at));
    assert_eq!(manual_of(state.figure(), 2, Dimension::Y), (-2.0, 1.0));
    assert!(state.figure().axes(NodeId(2)).unwrap().y.reverse);
}

// Why: links share limits only, so a pan on a reversed axis moves its unreversed partner to the same limits, and the
// partner keeps its own direction; the grabbed value stays under the pointer in the axes that was dragged.
#[test]
fn pan_on_a_reversed_axis_moves_a_linked_normal_axis_to_the_same_limits() {
    let mut figure = reversed_figure(false, false);
    let mut partner = figure.axes[0].clone();
    partner.id = NodeId(4);
    partner.cell.col = 1;
    partner.y.reverse = false;
    partner.artists.clear();
    figure.layout = ironlab_ir::TileLayout::new(1, 2);
    figure.axes.push(partner);
    figure.links.push(link(Dimension::Y, &[2, 4]));
    let (mut state, hit) = compiled(figure);
    let (_, y) = reversed_maps_of(&hit, false);
    let plot = hit.axes[0].plot_rect;
    let start = Point::new(plot.x + 0.5 * plot.width, plot.y + 0.5 * plot.height);
    let end = Point::new(start.x, start.y + 30.0);
    let grabbed = y.to_data(start.y);

    assert!(drag(&mut state, &hit, start, end));

    let limits = manual_of(state.figure(), 2, Dimension::Y);
    assert_eq!(manual_of(state.figure(), 4, Dimension::Y), limits);
    assert_close(
        remapped(y, limits).to_figure(grabbed),
        end.y,
        1e-6,
        "grabbed y",
    );
    assert!(!state.figure().axes(NodeId(4)).unwrap().y.reverse);
}

// Why: the property editor offers `reverse` beside the scale and limits of each axis, and its edit is an ordinary set
// recorded in the view overlay; the axis must then be drawn reversed, and undoing the edit must restore its direction.
#[test]
fn setting_reverse_in_the_overlay_reverses_the_axis_and_undo_restores_it() {
    let (mut state, hit) = compiled(reversed_figure(false, false));
    let (x, _) = reversed_maps_of(&hit, false);
    assert!(state.record(&set(2, "x.reverse", Value::Bool(true))));
    let (x_reversed, _) = reversed_maps_of(&recompiled(&state), true);
    assert_eq!((x_reversed.start, x_reversed.end), (x.end, x.start));
    assert!(state.undo());
    reversed_maps_of(&recompiled(&state), false);
    assert!(!state.source().axes(NodeId(2)).unwrap().x.reverse);
}

// ---------------------------------------------------------------------------------------------------------------
// A fixed aspect, kept while panning and zooming
// ---------------------------------------------------------------------------------------------------------------

/// A section-like closed curve spanning x ∈ [0, 1] and y ∈ [−0.06, 0.06] in axes 2 at equal aspect, with manual
/// limits x ∈ [−0.2, 1.2] and y ∈ [−0.4, 0.4] that do not match the shape of the plot area, so that a `Limits`
/// adjustment widens y and a `Box` adjustment shrinks the plot.
fn section_figure(adjust: AspectAdjust) -> Figure {
    let t: Vec<f64> = (0..=64).map(|k| f64::from(k) / 64.0 * TAU).collect();
    let x: Vec<f64> = t.iter().map(|t| 0.5 + 0.5 * t.cos()).collect();
    let y: Vec<f64> = t.iter().map(|t| 0.06 * t.sin()).collect();
    let mut figure = figure_of(
        vec![NdArray::vector(x), NdArray::vector(y)],
        vec![Artist::Series(Series {
            id: NodeId(3),
            x: array_id(0),
            y: array_id(1),
            ..Series::default()
        })],
    );
    let axes = figure.axes_mut(NodeId(2)).expect("the axes exists");
    axes.aspect = Aspect::Equal;
    axes.aspect_adjust = adjust;
    axes.x.limits = manual(-0.2, 1.2);
    axes.y.limits = manual(-0.4, 0.4);
    figure
}

/// Returns the drawn length of a unit of y over that of a unit of x in a 2D axes of a hit map.
fn drawn_ratio(hit: &HitMap, id: u64) -> f64 {
    let axes = hit
        .axes
        .iter()
        .find(|a| a.id == NodeId(id))
        .expect("the axes is drawn");
    let AxesHitKind::TwoD { x, y } = axes.kind else {
        panic!("axes {id} is two-dimensional");
    };
    let unit = |m: &AxisMap| (m.end - m.start).abs() / (m.max - m.min);
    unit(&y) / unit(&x)
}

#[track_caller]
fn assert_equal_aspect(hit: &HitMap, id: u64) {
    let ratio = drawn_ratio(hit, id);
    assert!((ratio - 1.0).abs() < 1e-9, "axes {id}: drawn ratio {ratio}");
}

/// Returns the data point drawn at `at` in axes 2 of a hit map.
fn data_at(hit: &HitMap, at: Point) -> (f64, f64) {
    let (x, y) = maps_of(hit);
    (x.to_data(at.x), y.to_data(at.y))
}

// Why: the wheel zooms about the pointer; at a fixed aspect kept by widening limits, the limits the gesture starts
// from are the widened ones the reader sees, not the narrower manual ones stored, or the first notch would jump the
// view. The limits a notch writes keep the data point under the pointer in the plot rectangle the reader zoomed in,
// and the aspect holds once the figure is drawn again.
#[test]
fn wheel_zoom_at_a_fixed_aspect_keeps_the_point_under_the_cursor_and_the_aspect() {
    for adjust in [AspectAdjust::Limits, AspectAdjust::Box] {
        let (mut state, mut hit) = compiled(section_figure(adjust));
        assert_equal_aspect(&hit, 2);
        for factor in [2.0, 1.5, 0.5] {
            let plot = hit.axes[0].plot_rect;
            let at = Point::new(plot.x + 0.3 * plot.width, plot.y + 0.35 * plot.height);
            let (x, y) = maps_of(&hit);
            let under = (x.to_data(at.x), y.to_data(at.y));
            assert!(state.scroll(&hit, at, factor));
            let (x, y) = (
                remapped(x, manual_of(state.figure(), 2, Dimension::X)),
                remapped(y, manual_of(state.figure(), 2, Dimension::Y)),
            );
            let what = format!("{adjust:?}, factor {factor}");
            assert_close(
                x.to_data(at.x),
                under.0,
                1e-9,
                &format!("{what}: x under the cursor"),
            );
            assert_close(
                y.to_data(at.y),
                under.1,
                1e-9,
                &format!("{what}: y under the cursor"),
            );
            hit = recompiled(&state);
            assert_equal_aspect(&hit, 2);
        }
    }
}

// Why: a pan grabs the data under the pointer and keeps it there; at a fixed aspect kept by widening limits, the
// grabbed point is read through the widened limits the reader sees, and the plot keeps its aspect after the pan.
#[test]
fn pan_at_a_fixed_aspect_keeps_the_grabbed_point_under_the_pointer() {
    for adjust in [AspectAdjust::Limits, AspectAdjust::Box] {
        let (mut state, hit) = compiled(section_figure(adjust));
        let plot = hit.axes[0].plot_rect;
        let from = Point::new(plot.x + 0.4 * plot.width, plot.y + 0.6 * plot.height);
        let to = Point::new(from.x + 30.0, from.y - 12.0);
        let grabbed = data_at(&hit, from);
        assert!(drag(&mut state, &hit, from, to));
        let (x, y) = maps_of(&hit);
        let (x, y) = (
            remapped(x, manual_of(state.figure(), 2, Dimension::X)),
            remapped(y, manual_of(state.figure(), 2, Dimension::Y)),
        );
        assert_close(
            x.to_data(to.x),
            grabbed.0,
            1e-9,
            &format!("{adjust:?}: x grabbed"),
        );
        assert_close(
            y.to_data(to.y),
            grabbed.1,
            1e-9,
            &format!("{adjust:?}: y grabbed"),
        );
        assert_equal_aspect(&recompiled(&state), 2);
    }
}

// Why: a rubber band of any shape must zoom to a view that keeps the aspect without hiding any of what the reader
// selected, so the shorter side of the band is expanded about its centre to the shape of the plot area; with `Box`
// this also keeps the plot area from changing shape at every zoom. A wide band grows in height; the tall band is
// tested below.
#[test]
fn a_rubber_band_at_a_fixed_aspect_expands_its_shorter_side_about_its_centre() {
    for adjust in [AspectAdjust::Limits, AspectAdjust::Box] {
        let (mut state, hit) = compiled(section_figure(adjust));
        state.tool = Tool::Zoom;
        let plot = hit.axes[0].plot_rect;
        // A wide, short band in the middle of the plot.
        let from = Point::new(plot.x + 0.2 * plot.width, plot.y + 0.45 * plot.height);
        let to = Point::new(plot.x + 0.7 * plot.width, plot.y + 0.55 * plot.height);
        let (x0, y0) = data_at(&hit, from);
        let (x1, y1) = data_at(&hit, to);
        state.drag_start(&hit, from);
        state.drag_update(to);
        let band = state.rubber_band().expect("the band is shown");
        assert_close(
            band.height / band.width,
            plot.height / plot.width,
            1e-9,
            "the band shown has the shape of the plot area",
        );
        assert_close(band.width, to.x - from.x, 1e-9, "the longer side is kept");
        assert!(state.drag_end(to));

        assert_equal_aspect(&recompiled(&state), 2);
        // The limits the zoom writes, before the figure is drawn again with tick labels that may reshape it.
        let (x, y) = (
            remapped(maps_of(&hit).0, manual_of(state.figure(), 2, Dimension::X)),
            remapped(maps_of(&hit).1, manual_of(state.figure(), 2, Dimension::Y)),
        );
        assert_close(x.min, x0.min(x1), 1e-9, "x min is the band's");
        assert_close(x.max, x0.max(x1), 1e-9, "x max is the band's");
        assert!(
            y.min < y0.min(y1) && y.max > y0.max(y1),
            "y shows all of the band"
        );
        assert_close(
            (y.min + y.max) / 2.0,
            (y0 + y1) / 2.0,
            1e-9,
            "y widens about the centre of the band",
        );
        // The new limits have the shape of the old ones, so a box keeps its shape and a plot whose limits are
        // widened needs no further widening.
        let (old_x, old_y) = maps_of(&hit);
        assert_close(
            (y.max - y.min) / (x.max - x.min),
            (old_y.max - old_y.min) / (old_x.max - old_x.min),
            1e-9,
            "the limits keep their shape",
        );
    }
}

// Why: a tall, narrow band must grow in width, not height: it is the shorter side relative to the plot's shape that
// expands. A band near the edge of the plot may expand past it, because the expanded band is the region the zoom
// shows, which may extend beyond the current view; clamping it back would hide part of what was selected.
#[test]
fn a_tall_rubber_band_at_a_fixed_aspect_expands_its_width_even_past_the_plot() {
    let (mut state, hit) = compiled(section_figure(AspectAdjust::Limits));
    state.tool = Tool::Zoom;
    let plot = hit.axes[0].plot_rect;
    let from = Point::new(plot.x + 0.02 * plot.width, plot.y + 0.1 * plot.height);
    let to = Point::new(plot.x + 0.06 * plot.width, plot.y + 0.9 * plot.height);
    state.drag_start(&hit, from);
    state.drag_update(to);
    let band = state.rubber_band().expect("the band is shown");
    assert_close(band.height, to.y - from.y, 1e-9, "the longer side is kept");
    assert_close(
        band.height / band.width,
        plot.height / plot.width,
        1e-9,
        "the band has the shape of the plot area",
    );
    assert!(
        band.x < plot.x,
        "the expanded band reaches past the plot: {band:?}"
    );
    let (x0, y0) = data_at(&hit, from);
    let (x1, y1) = data_at(&hit, to);
    assert!(state.drag_end(to));
    let (x, y) = (
        remapped(maps_of(&hit).0, manual_of(state.figure(), 2, Dimension::X)),
        remapped(maps_of(&hit).1, manual_of(state.figure(), 2, Dimension::Y)),
    );
    assert_close(y.min, y0.min(y1), 1e-9, "y min is the band's");
    assert_close(y.max, y0.max(y1), 1e-9, "y max is the band's");
    assert!(
        x.min < x0.min(x1) && x.max > x0.max(x1),
        "x shows all of the band"
    );
    assert_close(
        (x.min + x.max) / 2.0,
        (x0 + x1) / 2.0,
        1e-9,
        "x widens about its centre",
    );
    assert_equal_aspect(&recompiled(&state), 2);
}

/// Three linked panels of the same section at equal aspect, x and y linked over all of them, as on a family sheet.
fn linked_sections(adjust: AspectAdjust) -> Figure {
    let mut figure = section_figure(adjust);
    let template = figure.axes[0].clone();
    figure.layout = ironlab_ir::TileLayout::new(1, 3);
    figure.axes = (0..3)
        .map(|col| {
            let mut axes = template.clone();
            axes.id = NodeId(10 + u64::from(col));
            axes.cell.col = col;
            for (k, artist) in axes.artists.iter_mut().enumerate() {
                if let Artist::Series(line) = artist {
                    line.id = NodeId(100 + 10 * u64::from(col) + k as u64);
                }
            }
            axes
        })
        .collect();
    figure.links = vec![
        link(Dimension::X, &[10, 11, 12]),
        link(Dimension::Y, &[10, 11, 12]),
    ];
    figure
}

// Why: on a family sheet whose panels are linked in x and y at equal aspect, zooming one panel moves all of them,
// and every panel keeps its aspect. Zooming in and back out about one data point must return to the view the reader
// started from: exactly when the aspect is kept by the box, whose limits keep their shape under a zoom, and to within
// the reflow of tick labels when it is kept by widening limits, which may widen once more for a plot area that the
// new tick labels reshape. The view must never creep or oscillate from one gesture to the next.
#[test]
fn zooming_linked_panels_at_equal_aspect_moves_them_together_and_returns_without_drift() {
    for (adjust, tolerance) in [(AspectAdjust::Box, 1e-9), (AspectAdjust::Limits, 0.02)] {
        let (mut state, mut hit) = compiled(linked_sections(adjust));
        let anchor = {
            let AxesHitKind::TwoD { x, y } = hit.axes[1].kind else {
                panic!("two-dimensional");
            };
            let plot = hit.axes[1].plot_rect;
            (
                x.to_data(plot.x + 0.25 * plot.width),
                y.to_data(plot.y + 0.6 * plot.height),
            )
        };
        let start = hit.clone();
        for factor in [2.0, 2.0, 0.5, 0.5] {
            let AxesHitKind::TwoD { x, y } = hit.axes[1].kind else {
                panic!("two-dimensional");
            };
            let at = Point::new(x.to_figure(anchor.0), y.to_figure(anchor.1));
            assert!(state.scroll(&hit, at, factor));
            hit = recompiled(&state);
            let AxesHitKind::TwoD { x: x0, y: y0 } = hit.axes[0].kind else {
                panic!("two-dimensional");
            };
            for axes in &hit.axes {
                assert_equal_aspect(&hit, axes.id.0);
                let AxesHitKind::TwoD { x, y } = axes.kind else {
                    panic!("two-dimensional");
                };
                assert_eq!(
                    (x.min, x.max, y.min, y.max),
                    (x0.min, x0.max, y0.min, y0.max)
                );
            }
        }
        for (before, after) in start.axes.iter().zip(&hit.axes) {
            let (&AxesHitKind::TwoD { x: bx, y: by }, &AxesHitKind::TwoD { x: ax, y: ay }) =
                (&before.kind, &after.kind)
            else {
                panic!("two-dimensional");
            };
            for (b, a, span) in [
                (bx.min, ax.min, bx.max - bx.min),
                (bx.max, ax.max, bx.max - bx.min),
                (by.min, ay.min, by.max - by.min),
                (by.max, ay.max, by.max - by.min),
            ] {
                assert_close(
                    a,
                    b,
                    tolerance * span,
                    &format!("{adjust:?}: the view returns"),
                );
            }
        }
    }
}

/// A 3D axes (node 2) at equal aspect kept by `adjust`, with the manual limits of unequal span of
/// [`axes_3d`] (x ∈ [−1, 1], y ∈ [−2, 2], z ∈ [0, 3]), so that an automatic aspect would draw the
/// circle as an ellipse, holding a circle of radius 1 in
/// the plane z = 1 (node 3) and a vertical segment of length 2 at its centre (node 4).
fn circle_3d(adjust: AspectAdjust) -> Figure {
    let t: Vec<f64> = (0..=96).map(|k| f64::from(k) / 96.0 * TAU).collect();
    let mut figure = figure_of(
        vec![
            NdArray::vector(t.iter().map(|t| t.cos()).collect()),
            NdArray::vector(t.iter().map(|t| t.sin()).collect()),
            NdArray::vector(vec![1.0; t.len()]),
            NdArray::vector(vec![0.0, 0.0]),
            NdArray::vector(vec![0.5, 2.5]),
        ],
        vec![
            Artist::Series(Series {
                id: NodeId(3),
                x: array_id(0),
                y: array_id(1),
                z: Some(array_id(2)),
                ..Series::default()
            }),
            Artist::Series(Series {
                id: NodeId(4),
                x: array_id(3),
                y: array_id(3),
                z: Some(array_id(4)),
                ..Series::default()
            }),
        ],
    );
    let axes = figure.axes_mut(NodeId(2)).expect("the axes exists");
    *axes = Axes {
        artists: std::mem::take(&mut axes.artists),
        aspect: Aspect::Equal,
        aspect_adjust: adjust,
        ..axes_3d(2)
    };
    figure
}

/// Returns the box of the vertices drawn for an artist.
fn drawn_box(state: &FigureState, id: u64) -> Rect {
    let scene = ironlab_scene::compile(state.figure(), &TEXT);
    let mut points = Vec::new();
    scene.display_list.visit_leaves(|item, transform, _| {
        // The strokes of the line alone: a line in a 3D axes also fills a disc at each corner, which reaches half
        // the line width beyond the points it joins.
        if item.source == Some(NodeId(id))
            && let ironlab_scene::display::ItemKind::Path(path) = &item.kind
            && path.fill.is_none()
        {
            for segment in &path.segments {
                if let ironlab_scene::display::PathSegment::MoveTo(p)
                | ironlab_scene::display::PathSegment::LineTo(p) = segment
                {
                    points.push(transform.apply(*p));
                }
            }
        }
    });
    let (mut lo, mut hi) = (points[0], points[0]);
    for p in &points {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    Rect::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y)
}

// Why: the box of a 3D axes at a fixed aspect takes its proportions from its limits and the aspect alone, so rotating,
// zooming and panning the view in the viewer, which change the camera and never the limits, must never change them:
// seen from directly above, a circle in the xy plane is round however the view got there, and the ratio of its
// diameter to a vertical segment seen from the side is the same before and after a zoom.
#[test]
fn rotating_zooming_and_panning_a_3d_axes_never_changes_its_box_proportions() {
    for adjust in [AspectAdjust::Box, AspectAdjust::Limits] {
        let (mut state, hit) = compiled(circle_3d(adjust));
        let plot = hit.axes[0].plot_rect;
        let centre = Point::new(plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);

        // Rotate to look straight down: the elevation is clamped at 90 degrees.
        state.tool = Tool::Rotate;
        assert!(drag(
            &mut state,
            &hit,
            centre,
            Point::new(centre.x, centre.y + 400.0)
        ));
        assert_eq!(view_of(state.figure(), 2).elevation_deg, 90.0);
        let circle = drawn_box(&state, 3);
        assert_close(circle.width / circle.height, 1.0, 1e-3, "round from above");

        // Zooming and panning scale and move the drawing but keep its proportions.
        assert!(state.scroll(&hit, centre, 1.7));
        state.tool = Tool::Pan;
        assert!(drag(
            &mut state,
            &hit,
            centre,
            Point::new(centre.x + 15.0, centre.y - 9.0)
        ));
        let zoomed = drawn_box(&state, 3);
        assert_close(
            zoomed.width / zoomed.height,
            1.0,
            1e-3,
            "round after zoom and pan",
        );
        assert_close(zoomed.width / circle.width, 1.7, 1e-6, "the zoom scales it");

        // From the side the vertical segment of length 2 is as long as the diameter of the circle.
        state.tool = Tool::Rotate;
        assert!(drag(
            &mut state,
            &hit,
            centre,
            Point::new(centre.x, centre.y - 400.0)
        ));
        assert!(state.record(&set(
            2,
            "projection.view3d.elevation_deg",
            Value::Double(0.0)
        )));
        assert!(state.record(&set(2, "projection.view3d.azimuth_deg", Value::Double(0.0))));
        let (side, segment) = (drawn_box(&state, 3), drawn_box(&state, 4));
        assert_close(
            segment.height / side.width,
            1.0,
            1e-6,
            &format!("{adjust:?}: z is drawn as x is"),
        );
        // Seen from above and from the side, x runs to the right at the same scale: rotating changed nothing.
        assert_close(
            side.width,
            zoomed.width,
            1e-6,
            "the box keeps its size as it rotates",
        );
    }
}
