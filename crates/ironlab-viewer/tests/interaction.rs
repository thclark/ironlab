//! Pointer gestures as edits recorded in the view overlay, tested without a GPU or a window.
//!
//! Every gesture records a transaction of sets in the overlay and the viewer draws the composition of the source
//! figure with that overlay, so each test reads the displayed figure through [`FigureState::figure`] and the figure as
//! loaded through [`FigureState::source`].

mod common;

use common::*;
use ironlab_ir::{
    Artist, Axes, Color, DataId, Dimension, Figure, Image, ImagePlacement, IndexedImage, Limits,
    Line, MappedImage, NdArray, NodeId, OutOfRange, PixelRange, Scale, Text, Value, View3d,
};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{AxesHitKind, AxisMap, HitMap, LegendHit};
use ironlab_viewer::interaction::{MIN_BOX_ZOOM_POINTS, PixelDatatip, PixelValue, Tip};
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
            .map(|(k, array)| (DataId(k as u64), array))
            .collect(),
        axes: vec![Axes {
            id: NodeId(2),
            artists,
            ..Axes::default()
        }],
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

/// A colour-mapped image (node `id`) of the array `values`, placed by `placement`.
fn mapped_image(id: u64, values: u64, placement: ImagePlacement) -> Artist {
    Artist::MappedImage(MappedImage {
        id: NodeId(id),
        values: DataId(values),
        placement,
        ..MappedImage::default()
    })
}

/// The mapped image of `figure`, for a test that changes one of its properties before opening it.
fn mapped_image_mut(figure: &mut Figure, id: u64) -> &mut MappedImage {
    match figure.artist_mut(NodeId(id)) {
        Some(Artist::MappedImage(image)) => image,
        other => panic!("node {id} is a mapped image, not {other:?}"),
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
    let mut figure = figure_with_mapped_image(3, 4);
    mapped_image_mut(&mut figure, 3).display_name = Some(Text::plain("Heat"));
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
    assert_eq!(tip.name.as_deref(), Some("Heat"));

    let bytes =
        NdArray::from_shape_u8(vec![1, 2], vec![0, 255]).expect("the shape matches the values");
    let (state, hit) = compiled(figure_of(
        vec![bytes],
        vec![mapped_image(3, 0, ImagePlacement::default())],
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
        vec![mapped_image(
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
        vec![mapped_image(
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
        vec![mapped_image(3, 0, placed(Some((2.0, 7.0)), None))],
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
        vec![mapped_image(3, 0, ImagePlacement::default())],
    );
    figure.axes_mut(NodeId(2)).expect("axes exists").clim = manual(2.0, 3.0);
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

// Why: an indexed image looks its colormap up by the index as stored, and the datatip shows the reader that index:
// an 8-bit index as the number it is, and a floating-point index as stored rather than truncated, because 2.9 and
// 2 take the same entry but are different data, and it is the data the reader asked about.
#[test]
fn a_pixel_datatip_on_an_indexed_image_reports_the_stored_index_untruncated() {
    let indexed = |id: u64, indices: u64| {
        Artist::IndexedImage(IndexedImage {
            id: NodeId(id),
            indices: DataId(indices),
            ..IndexedImage::default()
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
        tip.name, None,
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
        "an index that truncates below the colormap, drawn transparent, is still reported as stored"
    );
}

// Why: a true-colour pixel has no single number, so the datatip shows its components, in the units the array
// stores them — bytes from 0 to 255, or fractions from 0 to 1 — with an alpha component when the array has one, so
// that what the reader sees is what their array holds and not a conversion they would have to undo.
#[test]
fn a_pixel_datatip_on_a_true_colour_image_reports_the_components_in_stored_units() {
    let image = |id: u64, pixels: u64| {
        Artist::Image(Image {
            id: NodeId(id),
            pixels: DataId(pixels),
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
            vec![mapped_image(3, 0, ImagePlacement::default())],
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
    mapped_image_mut(&mut hidden, 3).visible = false;
    let (state, hit) = compiled(hidden);
    assert!(
        state.pixel_datatip(&hit, at_data(&hit, 1.0, 1.0)).is_none(),
        "a hidden image"
    );

    let solid = Figure {
        id: NodeId(1),
        data: [(DataId(0), numbered(2, 2))].into(),
        axes: vec![Axes {
            artists: vec![mapped_image(3, 0, ImagePlacement::default())],
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
            mapped_image(3, 0, ImagePlacement::default()),
            mapped_image(4, 1, placed(Some((1.0, 2.0)), None)),
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
            mapped_image(3, 0, ImagePlacement::default()),
            Artist::Line(Line {
                id: NodeId(4),
                x: DataId(1),
                y: DataId(2),
                ..Line::default()
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

// Why: the property editor changes an out-of-range policy through the same overlay as every other edit, and the
// point of a policy is what the canvas then shows: setting `below` from transparent to a colour must make the
// recompiled image opaque in that colour where it was see-through and change nothing else, while the source figure
// keeps its own policy so that the change can be reverted.
#[test]
fn setting_the_below_policy_to_a_colour_recolours_the_transparent_pixels_of_the_recompiled_image() {
    // Values 0 and 1 lie below colour limits of [2, 3]; 2 and 3 lie at them.
    let mut figure = figure_of(
        vec![numbered(2, 2)],
        vec![mapped_image(3, 0, ImagePlacement::default())],
    );
    figure.axes_mut(NodeId(2)).expect("axes exists").clim = manual(2.0, 3.0);
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
            3,
            "below",
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
    match state.source().artist(NodeId(3)) {
        Some((_, Artist::MappedImage(image))) => assert_eq!(
            image.below,
            OutOfRange::Transparent,
            "the source keeps its own policy"
        ),
        other => panic!("node 3 of the source is the mapped image, not {other:?}"),
    }
}
