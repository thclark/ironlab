//! Pointer gestures as edits of the figure IR, tested without a GPU or a window.

mod common;

use common::*;
use ironlab_ir::{Artist, Dimension, Line, NodeId, Scale, Text};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{AxisMap, HitMap, LegendHit};
use ironlab_viewer::interaction::MIN_BOX_ZOOM_POINTS;
use ironlab_viewer::{FigureState, ROTATE_DEGREES_PER_POINT, Tool};

const PLOT: Rect = Rect::new(50.0, 20.0, 200.0, 100.0);

fn single_2d() -> (FigureState, HitMap) {
    let state = FigureState::new(figure_with(vec![axes_2d(2)], vec![]));
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![],
    };
    (state, hit)
}

fn single_3d() -> (FigureState, HitMap) {
    let state = FigureState::new(figure_with(vec![axes_3d(2)], vec![]));
    let hit = HitMap {
        axes: vec![hit_3d(2, PLOT)],
        legend_entries: vec![],
    };
    (state, hit)
}

/// The axis map that the compiler would produce for the current limits, keeping the figure-space extent of `old`.
fn remapped(old: AxisMap, (min, max): (f64, f64)) -> AxisMap {
    AxisMap { min, max, ..old }
}

// Why: zooming about the cursor is what lets a user inspect a feature without it sliding away; if the data point under
// the pointer moved, every wheel notch would need a compensating pan.
#[test]
fn wheel_zoom_keeps_the_data_point_under_the_cursor_on_linear_axes() {
    let (mut state, hit) = single_2d();
    let at = Point::new(100.0, 50.0);
    let (xm, ym) = (x_map(PLOT, 0.0, 10.0, false), y_map(PLOT, 0.0, 5.0, false));
    let (x0, y0) = (xm.to_data(at.x), ym.to_data(at.y));

    assert!(state.scroll(&hit, at, 2.0));

    let x = manual_of(&state.current, 2, Dimension::X);
    let y = manual_of(&state.current, 2, Dimension::Y);
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
    let x = manual_of(&state.current, 2, Dimension::X);
    let y = manual_of(&state.current, 2, Dimension::Y);
    let zoomed = HitMap {
        axes: vec![ironlab_scene::hit::AxesHit {
            kind: ironlab_scene::hit::AxesHitKind::TwoD {
                x: x_map(PLOT, x.0, x.1, false),
                y: y_map(PLOT, y.0, y.1, false),
            },
            ..hit_2d(2, PLOT)
        }],
        legend_entries: vec![],
    };

    assert!(state.scroll(&zoomed, at, 1.0 / 1.25));

    let x = manual_of(&state.current, 2, Dimension::X);
    let y = manual_of(&state.current, 2, Dimension::Y);
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
    };
    let at = Point::new(100.0, 70.0);
    let x0 = xm.to_data(at.x);

    assert!(state.scroll(&hit, at, 2.0));
    let x = manual_of(&state.current, 2, Dimension::X);
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
    };
    assert!(state.scroll(&zoomed, at, 0.1));
    let x = manual_of(&state.current, 2, Dimension::X);
    assert!(
        x.0 > 0.0,
        "log limits stay positive after zooming out, got {x:?}"
    );
}

// Why: automatic limits are recomputed from the data on every compile, so a zoom that left them Auto would be undone
// immediately; the zoom must start from the limits the user was looking at and pin them.
#[test]
fn wheel_zoom_on_automatic_limits_writes_manual_limits_from_the_resolved_ones() {
    let mut figure = figure_with(vec![axes_2d(2)], vec![]);
    let axes = figure.axes_mut(NodeId(2)).unwrap();
    axes.x.limits = ironlab_ir::Limits::Auto;
    axes.y.limits = ironlab_ir::Limits::Auto;
    let mut state = FigureState::new(figure);
    let hit = HitMap {
        axes: vec![hit_2d(2, PLOT)],
        legend_entries: vec![],
    };
    let centre = Point::new(PLOT.x + PLOT.width / 2.0, PLOT.y + PLOT.height / 2.0);

    assert!(state.scroll(&hit, centre, 2.0));

    let x = manual_of(&state.current, 2, Dimension::X);
    let y = manual_of(&state.current, 2, Dimension::Y);
    assert_close(x.0, 2.5, EPS, "x min");
    assert_close(x.1, 7.5, EPS, "x max");
    assert_close(y.0, 1.25, EPS, "y min");
    assert_close(y.1, 3.75, EPS, "y max");
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
    let x = manual_of(&state.current, 2, Dimension::X);
    let y = manual_of(&state.current, 2, Dimension::Y);

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
            manual_of(&direct.current, 2, Dimension::X),
            manual_of(&direct.current, 2, Dimension::Y)
        ),
        "many small updates give exactly the same limits as one update to the same point"
    );

    state.drag_end(end);
    assert_eq!(
        manual_of(&state.current, 2, Dimension::X),
        x,
        "releasing does not move the view"
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
    };
    let start = Point::new(120.0, 90.0);
    let end = Point::new(120.0, 40.0);
    let grab = ym.to_data(start.y);

    state.drag_start(&hit, start);
    assert!(state.drag_update(end));

    let y = manual_of(&state.current, 2, Dimension::Y);
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
    };

    state.drag_start(&hit, Point::new(60.0, 60.0));
    assert!(state.drag_update(Point::new(100.0, 80.0)));

    let a_x = manual_of(&state.current, 2, Dimension::X);
    assert_ne!(a_x, (0.0, 10.0), "the dragged axes moved in x");
    assert_ne!(
        manual_of(&state.current, 2, Dimension::Y),
        (0.0, 5.0),
        "the dragged axes moved in y"
    );
    assert_eq!(
        manual_of(&state.current, 3, Dimension::X),
        a_x,
        "the x-linked partner follows in x"
    );
    assert_eq!(
        limits_of(&state.current, 3, Dimension::Y),
        limits_of(&before, 3, Dimension::Y),
        "the partner is not linked in y"
    );
    assert_eq!(
        state.current.axes(NodeId(4)),
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
    let before = state.current.clone();
    let start = Point::new(150.0, 70.0);

    state.drag_start(&hit, start);
    assert!(state.drag_update(Point::new(start.x + 10.0, start.y + 4.0)));
    let view = view_of(&state.current, 2);
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
        view_of(&state.current, 2).elevation_deg,
        90.0,
        EPS,
        "elevation clamps at +90",
    );
    state.drag_update(Point::new(start.x, start.y - 1000.0));
    assert_close(
        view_of(&state.current, 2).elevation_deg,
        -90.0,
        EPS,
        "elevation clamps at -90",
    );

    state.drag_end(Point::new(start.x, start.y - 1000.0));
    let axes = state.current.axes(NodeId(2)).unwrap();
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
    let before = state.current.clone();

    assert!(state.scroll(&hit, Point::new(150.0, 70.0), 2.0));

    assert_close(view_of(&state.current, 2).zoom, 2.0, EPS, "zoom doubles");
    let axes = state.current.axes(NodeId(2)).unwrap();
    let old = before.axes(NodeId(2)).unwrap();
    assert_eq!((&axes.x, &axes.y, &axes.z), (&old.x, &old.y, &old.z));
}

// Why: panning a 3D axes moves the camera offset, which the scene compiler interprets as fractions of the plot
// rectangle; the direction contract (positive pan[1] is downwards) must match the compiler for the box to follow the
// pointer.
#[test]
fn pan_on_3d_axes_moves_the_view_offset_by_the_pointer_displacement_in_plot_fractions() {
    let (mut state, hit) = single_3d();
    let before = state.current.clone();

    state.drag_start(&hit, Point::new(100.0, 50.0));
    assert!(state.drag_update(Point::new(120.0, 60.0)));

    let view = view_of(&state.current, 2);
    assert_close(view.pan[0], 20.0 / PLOT.width, EPS, "pan x");
    assert_close(view.pan[1], 10.0 / PLOT.height, EPS, "pan y");
    let axes = state.current.axes(NodeId(2)).unwrap();
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
        let x = manual_of(&state.current, 2, Dimension::X);
        let y = manual_of(&state.current, 2, Dimension::Y);
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

    let x = manual_of(&state.current, 2, Dimension::X);
    let y = manual_of(&state.current, 2, Dimension::Y);
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
    let before = state.current.clone();

    state.drag_start(&hit, Point::new(100.0, 40.0));
    assert!(!state.drag_update(Point::new(150.0, 90.0)));
    assert_eq!(state.rubber_band(), None, "no band is drawn over 3D axes");
    assert!(!state.drag_end(Point::new(150.0, 90.0)));
    assert_eq!(state.current, before);

    for tool in [Tool::Pan, Tool::Zoom, Tool::Rotate] {
        let (mut state, hit) = single_3d();
        state.tool = tool;
        assert!(state.scroll(&hit, Point::new(150.0, 70.0), 2.0), "{tool:?}");
        assert_close(view_of(&state.current, 2).zoom, 2.0, EPS, "3D zoom");

        let (mut state, hit) = single_2d();
        state.tool = tool;
        assert!(state.scroll(&hit, Point::new(150.0, 70.0), 2.0), "{tool:?}");
        let x = manual_of(&state.current, 2, Dimension::X);
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
        let before = state.current.clone();

        state.drag_start(&hit, Point::new(100.0, 40.0));
        state.drag_update(to);
        assert!(!state.drag_end(to), "band to {to:?} is ignored");
        assert_eq!(state.current, before);
    }
}

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
    };
    for (id, x, y) in [
        (2, (100.0, 110.0), (7.0, 8.0)),
        (3, (100.0, 110.0), (50.0, 60.0)),
        (4, (-3.0, 3.0), (-1.0, 1.0)),
    ] {
        let axes = state.current.axes_mut(NodeId(id)).unwrap();
        axes.x.limits = manual(x.0, x.1);
        axes.y.limits = manual(y.0, y.1);
    }

    assert!(state.double_click(&hit, Point::new(300.0, 60.0)));
    assert_eq!(manual_of(&state.current, 4, Dimension::X), (0.0, 10.0));
    assert_eq!(manual_of(&state.current, 4, Dimension::Y), (0.0, 5.0));
    assert_eq!(
        manual_of(&state.current, 2, Dimension::X),
        (100.0, 110.0),
        "other axes keep their view"
    );

    assert!(state.double_click(&hit, Point::new(60.0, 60.0)));
    assert_eq!(manual_of(&state.current, 2, Dimension::X), (0.0, 10.0));
    assert_eq!(manual_of(&state.current, 2, Dimension::Y), (0.0, 5.0));
    assert_eq!(
        manual_of(&state.current, 3, Dimension::X),
        (0.0, 10.0),
        "x-linked partner follows"
    );
    assert_eq!(
        manual_of(&state.current, 3, Dimension::Y),
        (50.0, 60.0),
        "partner is not linked in y"
    );
}

// Why: double-click on a rotated 3D axes is how a user gets back to the as-loaded camera without resetting every
// subplot.
#[test]
fn double_click_restores_the_view_of_a_3d_axes() {
    let (mut state, hit) = single_3d();
    set_view(
        &mut state.current,
        2,
        ironlab_ir::View3d {
            azimuth_deg: 10.0,
            elevation_deg: -20.0,
            zoom: 3.0,
            pan: [0.2, -0.1],
        },
    );

    assert!(state.double_click(&hit, Point::new(150.0, 70.0)));
    assert_eq!(view_of(&state.current, 2), view_of(&state.snapshot, 2));
}

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
    };
    (state, hit)
}

fn visible(state: &FigureState, id: u64) -> bool {
    state.current.artist(NodeId(id)).unwrap().1.visible()
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
    state.current.axes_mut(NodeId(2)).unwrap().x.limits = navigated;
    let entry = Point::new(220.0, 30.0);

    assert!(state.click(&hit, entry), "the first click hides the series");
    assert!(!visible(&state, 10));
    assert!(
        state.double_click(&hit, entry),
        "the second click shows it again"
    );
    assert!(visible(&state, 10));
    assert_eq!(
        state.current.axes(NodeId(2)).unwrap().x.limits,
        navigated,
        "a double-click on a legend entry does not reset the axes"
    );

    assert!(
        state.double_click(&hit, Point::new(100.0, 100.0)),
        "a double-click in the plot area still resets the axes"
    );
    assert_ne!(state.current.axes(NodeId(2)).unwrap().x.limits, navigated);
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
    let snapshot = state.snapshot.clone();
    {
        let a = state.current.axes_mut(NodeId(2)).unwrap();
        a.x.limits = manual(3.0, 4.0);
        a.y.limits = ironlab_ir::Limits::Auto;
    }
    state.current.axes_mut(NodeId(3)).unwrap().z.limits = manual(-9.0, 9.0);
    set_view(
        &mut state.current,
        3,
        ironlab_ir::View3d {
            azimuth_deg: 45.0,
            elevation_deg: 10.0,
            zoom: 0.5,
            pan: [0.1, 0.1],
        },
    );
    state
        .current
        .artist_mut(NodeId(10))
        .unwrap()
        .set_visible(false);

    assert!(state.reset_view());

    for id in [2, 3] {
        for dim in [Dimension::X, Dimension::Y, Dimension::Z] {
            assert_eq!(
                limits_of(&state.current, id, dim),
                limits_of(&snapshot, id, dim),
                "axes {id} {dim:?}"
            );
        }
    }
    assert_eq!(view_of(&state.current, 3), view_of(&snapshot, 3));
    assert!(!visible(&state, 10), "visibility is kept");
    assert!(
        !state.reset_view(),
        "resetting an already reset view changes nothing"
    );
}

// Why: the canvas forwards every pointer event; events over margins, titles or empty figure space must not edit the
// figure or trigger a recompile.
#[test]
fn events_outside_every_axes_do_nothing_and_report_no_change() {
    let (mut state, hit) = figure_with_legend();
    let outside = Point::new(5.0, 5.0);
    let before = state.current.clone();

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
    assert_eq!(state.current, before);
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
    let before = state.current.clone();

    state.drag_start(&hit, Point::new(100.0, 50.0));
    assert!(!state.drag_update(Point::new(160.0, 90.0)));
    assert!(!state.drag_end(Point::new(160.0, 90.0)));

    assert_eq!(state.current, before);
}
