//! The host-agnostic figure canvas, exercised without a window.
//!
//! [`FigureCanvas`] holds what `FigurePane` used to mix with egui: the fit of the figure into an area, the draw list
//! kept for that fit, the conversion of decided gestures into figure-space calls on [`FigureState`], and the chrome
//! (cursor, rubber band, callout) a host draws. [`Pointer`] turns raw pointer events into the gestures egui would
//! decide, with egui's own thresholds, so that a browser host reaches the same decisions as the egui host.
//!
//! Most tests here are the pure twin of a kittest test in `app.rs`, which drives the same logic through egui and
//! remains the proof that the egui host forwards to it; the twin is named in each `Why`.

mod common;

use std::sync::Arc;

use common::*;
use emath::{Pos2, Rect, pos2, vec2};
use ironlab_canvas::{
    CANVAS_MARGIN, Callout, Cursor, DATATIP_RING_POINTS, FigureCanvas, Fit, Gesture, MAX_TILE_SIDE,
    Marker, Origin, Pointer, REBUILD_RATIO, Tip, Tool, WHEEL_ZOOM_RATE, datatip_text,
    pixel_datatip_text, wheel_factor,
};
use ironlab_ir::{Dimension, FigureSize, NodeId, Value};
use ironlab_scene::display::Point;
use ironlab_scene::hit::{AxesHitKind, AxisMap, HitMap};

/// The area the kittest twins fit their figure into: a 900 × 600 harness. It is placed away from the origin so that
/// a test which forgot the area's position would fail.
fn area() -> Rect {
    Rect::from_min_size(pos2(10.0, 20.0), vec2(900.0, 600.0))
}

/// A canvas of `figure` fitted into [`area`], with its fit.
fn fitted(figure: ironlab_ir::Figure) -> (FigureCanvas, Fit) {
    let mut canvas = FigureCanvas::new(figure);
    canvas.resize(area(), MAX_TILE_SIDE);
    let fit = canvas
        .fit(&TEXT)
        .expect("a figure with a page fits into a non-empty area");
    (canvas, fit)
}

/// The hit map of the canvas's current scene, cloned so that the canvas can be driven while it is held.
fn hit_map_of(canvas: &mut FigureCanvas) -> HitMap {
    canvas.scene(&TEXT).hit_map.clone()
}

/// The horizontal and vertical axis maps of the first axes of the hit map, which is two-dimensional.
fn maps_of(hit: &HitMap) -> (AxisMap, AxisMap) {
    match &hit.axes.first().expect("the figure has an axes").kind {
        AxesHitKind::TwoD { x, y } => (*x, *y),
        AxesHitKind::ThreeD => panic!("the first axes is two-dimensional"),
    }
}

/// The screen position at fractions `(fx, fy)` across and down the plot rectangle of the first axes.
fn plot_point(hit: &HitMap, fit: &Fit, fx: f64, fy: f64) -> Pos2 {
    let plot = hit.axes.first().expect("the figure has an axes").plot_rect;
    fit.to_screen.apply(Point::new(
        plot.x + fx * plot.width,
        plot.y + fy * plot.height,
    ))
}

#[track_caller]
fn assert_pos_close(actual: Pos2, expected: Pos2, tolerance: f32, what: &str) {
    assert!(
        actual.distance(expected) <= tolerance,
        "{what}: expected {expected:?}, got {actual:?} (tolerance {tolerance})"
    );
}

/// A pan of the first axes by `dx` screen points to the right, from a point well inside its plot, as one closed drag.
fn pan_right(canvas: &mut FigureCanvas, fit: &Fit, dx: f32) {
    let hit = hit_map_of(canvas);
    let from = plot_point(&hit, fit, 0.3, 0.4);
    canvas.drag_start(from, &TEXT);
    assert!(canvas.drag_move(from + vec2(dx, 0.0)));
    canvas.drag_end(Some(from + vec2(dx, 0.0)));
}

// ---------------------------------------------------------------------------------------------------------------------
// The fit
// ---------------------------------------------------------------------------------------------------------------------

// Why: every gesture and every mark is converted through the fit, so the fit must be exactly the one the egui canvas
// computes today — the page scaled uniformly to fit inside the margin and centred in the area — or a hover would read
// the wrong point and a pan would move by the wrong amount; and the transform must round-trip, or the screen-to-figure
// conversion of a gesture would not land where the figure-to-screen conversion of a mark did. Twin: the geometry the
// `CANVAS_CENTRE` of every canvas test in `app.rs` relies on.
#[test]
fn the_figure_is_fitted_and_centred_in_the_area_inside_the_margin() {
    // Five inches by four: 360 × 288 points, exactly, so that the expected scale is a plain quotient.
    let mut figure = figure_with(vec![axes_2d(2)], vec![]);
    figure.size = FigureSize {
        width_mm: 127.0,
        height_mm: 101.6,
    };
    let (mut canvas, fit) = fitted(figure);
    let (width_pt, height_pt) = {
        let list = &canvas.scene(&TEXT).display_list;
        (list.width_pt as f32, list.height_pt as f32)
    };
    assert_close(f64::from(width_pt), 360.0, 1e-6, "the page width in points");
    assert_close(
        f64::from(height_pt),
        288.0,
        1e-6,
        "the page height in points",
    );

    let area = area();
    let expected_scale = ((area.width() - 2.0 * CANVAS_MARGIN) / width_pt)
        .min((area.height() - 2.0 * CANVAS_MARGIN) / height_pt);
    assert_close(
        f64::from(fit.to_screen.scale),
        f64::from(expected_scale),
        1e-6,
        "the scale is the smaller of the two that fit inside the margin",
    );
    assert!(
        fit.page.width() <= area.width() - 2.0 * CANVAS_MARGIN + 1e-3
            && fit.page.height() <= area.height() - 2.0 * CANVAS_MARGIN + 1e-3,
        "the page lies inside the margin: {:?} in {area:?}",
        fit.page
    );
    assert_pos_close(
        fit.page.center(),
        area.center(),
        1e-3,
        "the page is centred in the area",
    );
    assert_pos_close(
        fit.page.size().to_pos2(),
        pos2(width_pt * expected_scale, height_pt * expected_scale),
        1e-3,
        "the page is the figure at that scale",
    );
    assert_pos_close(
        fit.to_screen.origin,
        fit.page.min,
        1e-6,
        "the transform puts the figure's top-left corner at the page's",
    );
    assert_pos_close(
        fit.to_screen.origin,
        area.center() - fit.page.size() / 2.0,
        1e-3,
        "the origin is the area's centre less half the page, as the egui canvas computes it",
    );
    for p in [
        Point::new(0.0, 0.0),
        Point::new(360.0, 288.0),
        Point::new(123.456, 7.89),
    ] {
        let back = fit.to_screen.invert(fit.to_screen.apply(p));
        assert_close(back.x, p.x, 1e-3, "x survives a round trip through the fit");
        assert_close(back.y, p.y, 1e-3, "y survives a round trip through the fit");
    }
}

// Why: a canvas that is not yet laid out, collapsed to nothing, or too small for its margin has nowhere to put the
// figure; the egui canvas draws nothing in those cases and reads no input, and the controller must report the same
// rather than a fit of zero or infinite scale that every gesture would then divide by. A figure of no size, on the
// other hand, is not left without a page: the scene compiler draws it at the default size and warns, as ADR 0012
// decides, so it fits like any other and the warning reaches the problems the host shows.
#[test]
fn there_is_no_fit_for_an_empty_area_but_a_figure_of_no_size_takes_the_default_page() {
    let mut canvas = FigureCanvas::new(figure_with(vec![axes_2d(2)], vec![]));
    for (what, area) in [
        (
            "an area of no width",
            Rect::from_min_size(pos2(10.0, 20.0), vec2(0.0, 600.0)),
        ),
        (
            "an area of no height",
            Rect::from_min_size(pos2(10.0, 20.0), vec2(900.0, 0.0)),
        ),
        (
            "an area no wider than its two margins",
            Rect::from_min_size(pos2(10.0, 20.0), vec2(2.0 * CANVAS_MARGIN, 600.0)),
        ),
    ] {
        canvas.resize(area, MAX_TILE_SIDE);
        assert!(canvas.fit(&TEXT).is_none(), "{what} gives no fit");
        assert!(
            canvas.draw_list(&TEXT).is_none(),
            "{what} gives no draw list either"
        );
    }
    canvas.resize(area(), MAX_TILE_SIDE);
    assert!(
        canvas.fit(&TEXT).is_some(),
        "the same canvas fits once it has room"
    );

    let mut sizeless = figure_with(vec![], vec![]);
    sizeless.size = FigureSize {
        width_mm: 0.0,
        height_mm: 0.0,
    };
    let mut canvas = FigureCanvas::new(sizeless);
    canvas.resize(area(), MAX_TILE_SIDE);
    let fit = canvas
        .fit(&TEXT)
        .expect("a figure of no size is drawn at the default size");
    assert!(
        fit.to_screen.scale.is_finite() && fit.to_screen.scale > 0.0,
        "and fits at a real scale: {fit:?}"
    );
    let problems = canvas.problems(&TEXT);
    assert!(
        problems
            .iter()
            .any(|p| p.origin == Origin::Scene && p.detail.contains("not usable")),
        "the substitution is reported as a warning of the scene: {problems:?}"
    );
}

// Why: the page is filled with the figure's background before the draw list is painted over it, so the host needs
// the colour in the form its painter takes (premultiplied bytes), and nothing at all when the background is fully
// transparent so that it can leave the surround showing through, as the egui canvas does today.
#[test]
fn the_background_is_reported_premultiplied_and_omitted_when_transparent() {
    let (mut canvas, _) = fitted(figure_with(vec![axes_2d(2)], vec![]));
    assert_eq!(
        canvas.background(&TEXT),
        Some([255, 255, 255, 255]),
        "the default figure background is opaque white"
    );

    let mut figure = figure_with(vec![axes_2d(2)], vec![]);
    figure.background = ironlab_ir::Color::rgba(0.0, 0.0, 0.0, 0.0);
    let (mut canvas, _) = fitted(figure);
    assert_eq!(
        canvas.background(&TEXT),
        None,
        "a fully transparent background is not painted"
    );
}

// Why: the native window keeps a margin between the figure and the edges of its canvas so that the page reads as a
// page on a surround, but a figure embedded in a document fills the box the document gives it, as an image would; a
// host must therefore be able to fit the page with no margin, and, since the box is sized to the page's aspect ratio,
// the page must then cover the area exactly, so that nothing of the host's backdrop shows through.
#[test]
fn a_host_may_fit_the_page_with_no_margin_so_that_it_fills_an_area_of_its_own_aspect_ratio() {
    let mut figure = figure_with(vec![axes_2d(2)], vec![]);
    figure.size = FigureSize {
        width_mm: 127.0,
        height_mm: 101.6,
    };
    let mut canvas = FigureCanvas::new(figure);
    canvas.set_margin(0.0);
    // Twice the page, so that the fit is exact rather than a rounded quotient.
    let area = Rect::from_min_size(pos2(10.0, 20.0), vec2(720.0, 576.0));
    canvas.resize(area, MAX_TILE_SIDE);
    let fit = canvas.fit(&TEXT).expect("the page fits");
    assert_close(
        f64::from(fit.to_screen.scale),
        2.0,
        1e-6,
        "the page is scaled to the area",
    );
    assert_pos_close(
        fit.page.min,
        area.min,
        1e-3,
        "the page starts at the area's corner",
    );
    assert_pos_close(fit.page.max, area.max, 1e-3, "and ends at its far corner");

    canvas.set_margin(CANVAS_MARGIN);
    let fit = canvas.fit(&TEXT).expect("the page still fits");
    assert!(
        fit.page.width() < area.width() && fit.page.min.x > area.min.x,
        "restoring the margin fits the page inside it again"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// The draw list
// ---------------------------------------------------------------------------------------------------------------------

// Why: the host uploads a list once and draws it from its buffers while it is handed the same `Arc`, so an idle
// frame and a small resize must return the very same list, or every frame would upload the figure and every resize
// would stutter; the geometry is flattened for the scale it was built at, so a resize beyond REBUILD_RATIO must
// yield a new list, and an edit changes the scene the list was built from, so it must too. Twin:
// `the_draw_list_is_kept_through_idle_frames_and_small_resizes_but_not_large_ones_or_edits` in `app.rs`, whose
// factors of the window's initial size these are.
#[test]
fn the_draw_list_is_kept_across_small_resizes_and_rebuilt_across_large_ones_and_edits() {
    let (mut canvas, fit) = fitted(figure_with(vec![axes_2d(2)], vec![]));
    let initial = area();
    let (first, transform) = canvas
        .draw_list(&TEXT)
        .expect("a fitted figure has a draw list");
    assert_eq!(
        transform, fit.to_screen,
        "the list is handed back with the transform of the fit it was built for"
    );
    assert!(
        canvas
            .built_list()
            .is_some_and(|built| Arc::ptr_eq(&built, &first)),
        "the built list is the one just handed out"
    );

    let mut previous = first;
    // Each step follows the ones before it; a resize is a factor of the area's initial size, as the twin's is of the
    // window's, so the second step moves the scale by 1.5 / 1.1 ≈ 1.36 from the scale the list was built at.
    for (what, resize, kept) in [
        ("an idle frame", None, true),
        ("a resize of the area by 10 %", Some(1.1_f32), true),
        ("a resize of the area by 50 %", Some(1.5), false),
    ] {
        if let Some(factor) = resize {
            canvas.resize(
                Rect::from_min_size(initial.min, initial.size() * factor),
                MAX_TILE_SIDE,
            );
        }
        let fit = canvas.fit(&TEXT).expect("the area is never empty");
        let (current, transform) = canvas
            .draw_list(&TEXT)
            .expect("the figure has a draw list after every step");
        assert_eq!(
            Arc::ptr_eq(&previous, &current),
            kept,
            "after {what} the list is {}",
            if kept { "kept" } else { "rebuilt" }
        );
        assert_eq!(
            transform, fit.to_screen,
            "after {what} the list is drawn with the current fit, whether or not it was rebuilt"
        );
        previous = current;
    }

    let scale_before = canvas.fit(&TEXT).expect("fitted").to_screen.scale;
    let ratio_within = REBUILD_RATIO.sqrt();
    canvas.resize(
        Rect::from_min_size(initial.min, initial.size() * 1.5 / ratio_within),
        MAX_TILE_SIDE,
    );
    let scale_after = canvas.fit(&TEXT).expect("fitted").to_screen.scale;
    assert!(
        scale_after < scale_before && scale_after > scale_before / REBUILD_RATIO,
        "precondition: the area shrank by less than the ratio ({scale_before} → {scale_after})"
    );
    let (shrunk, _) = canvas.draw_list(&TEXT).expect("fitted");
    assert!(
        Arc::ptr_eq(&previous, &shrunk),
        "a shrink within the ratio keeps the list, as a growth within it does"
    );

    record_limits(canvas.state_mut(), 2, Dimension::X, manual(3.0, 4.0));
    canvas.invalidate();
    let (edited, _) = canvas
        .draw_list(&TEXT)
        .expect("the figure has a draw list after the edit");
    assert!(
        !Arc::ptr_eq(&shrunk, &edited),
        "after an edit the list is rebuilt from the new scene"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Gestures
// ---------------------------------------------------------------------------------------------------------------------

// Why: the interaction logic is tested in figure space, so the glue — converting the pointer through the fit and
// hit-testing against the current compilation — is what the controller adds; a wheel over the plot must zoom about
// the point under the pointer exactly as `FigureState::scroll` does when given the fit's inversion of that point,
// which proves the conversion is the fit's and not, say, the area's. Twin:
// `scrolling_up_over_the_canvas_zooms_the_figure_in` in `app.rs`.
#[test]
fn a_wheel_over_the_plot_zooms_about_the_point_under_the_pointer() {
    let (mut canvas, fit) = fitted(figure_with(vec![axes_2d(2)], vec![]));
    let hit = hit_map_of(&mut canvas);
    let pos = plot_point(&hit, &fit, 0.3, 0.6);
    let factor = wheel_factor(50.0, 1.0);
    assert!(factor > 1.0, "scrolling up zooms in");

    let mut twin = canvas.state().clone();
    assert!(twin.scroll(&hit, fit.to_screen.invert(pos), factor));

    assert!(
        canvas.wheel(pos, factor, &TEXT),
        "a wheel notch over the plot changes the figure"
    );
    for dimension in [Dimension::X, Dimension::Y] {
        let (got, want) = (
            manual_of(canvas.state().figure(), 2, dimension),
            manual_of(twin.figure(), 2, dimension),
        );
        assert_close(got.0, want.0, 1e-9, "the lower limit is the state's own");
        assert_close(got.1, want.1, 1e-9, "the upper limit is the state's own");
    }
    let x = manual_of(canvas.state().figure(), 2, Dimension::X);
    let y = manual_of(canvas.state().figure(), 2, Dimension::Y);
    assert!(
        x.0 > 0.0 && x.1 < 10.0 && y.0 > 0.0 && y.1 < 5.0,
        "the zoom is about a point inside the plot, so both limits move inwards: x {x:?}, y {y:?}"
    );

    let before = canvas.state().figure().clone();
    assert!(
        !canvas.wheel(pos, 1.0, &TEXT),
        "a factor of one is not a zoom"
    );
    assert_eq!(canvas.state().figure(), &before, "and changes nothing");
    assert!(
        !canvas.wheel(area().min + vec2(1.0, 1.0), factor, &TEXT),
        "a wheel outside the figure zooms nothing"
    );
    assert_eq!(canvas.state().figure(), &before);
}

// Why: a drag with the Pan tool must move the data with the pointer by the distance dragged, converted through the
// fit; the host may lose the pointer before the button is released (it left the window, or a touch ended), and the
// egui canvas then ends the drag with no position, which `FigureState` reads as NaN and closes the drag without
// moving it further. Whatever the ending, the drag must be closed: no band, the resting cursor, and one undo step
// that restores the original limits. Twin: `dragging_right_over_the_canvas_pans_the_data_to_the_right` in `app.rs`.
#[test]
fn a_drag_pans_and_ending_it_without_a_position_closes_the_drag() {
    let (mut canvas, fit) = fitted(figure_with(vec![axes_2d(2)], vec![]));
    let hit = hit_map_of(&mut canvas);
    let (x_map, _) = maps_of(&hit);
    let from = plot_point(&hit, &fit, 0.3, 0.4);
    let to = from + vec2(60.0, 0.0);
    assert_eq!(
        canvas.cursor(),
        Cursor::Grab,
        "the Pan tool rests as a hand"
    );

    canvas.drag_start(from, &TEXT);
    assert_eq!(
        canvas.cursor(),
        Cursor::Grabbing,
        "the hand closes while dragging"
    );
    assert!(canvas.drag_move(to), "the pan changes the figure");

    // Sixty screen points is 60 / scale figure points, which the axis map turns into data.
    let shift = {
        let start = fit.to_screen.invert(from).x;
        let end = fit.to_screen.invert(to).x;
        x_map.to_data(end) - x_map.to_data(start)
    };
    let x = manual_of(canvas.state().figure(), 2, Dimension::X);
    let y = manual_of(canvas.state().figure(), 2, Dimension::Y);
    assert_close(
        x.0,
        -shift,
        1e-9,
        "the data moved right, so the x limits decreased by the drag",
    );
    assert_close(x.1, 10.0 - shift, 1e-9, "panning keeps the x range");
    assert_close(y.0, 0.0, 1e-9, "a horizontal drag leaves y alone");
    assert_close(y.1, 5.0, 1e-9, "a horizontal drag leaves y alone");
    assert!(
        x.0 < 0.0 && x.1 < 10.0,
        "and the shift is a real distance: {x:?}"
    );

    // The twin is driven as the egui canvas drives it when the pointer is gone: a drag end at NaN.
    let mut twin = canvas.state().clone();
    let twin_changed = twin.drag_end(Point::new(f64::NAN, f64::NAN));
    assert_eq!(
        canvas.drag_end(None),
        twin_changed,
        "ending without a position reports what the state reports for NaN"
    );
    assert_eq!(
        manual_of(canvas.state().figure(), 2, Dimension::X),
        x,
        "the limits stay where the last move put them"
    );
    assert_eq!(canvas.cursor(), Cursor::Grab, "the drag is over");
    assert!(canvas.rubber_band(&TEXT).is_none());
    assert!(canvas.can_undo(), "the whole drag is one undo step");
    assert!(!canvas.can_redo());
    assert!(canvas.undo());
    assert_eq!(
        manual_of(canvas.state().figure(), 2, Dimension::X),
        (0.0, 10.0),
        "one undo restores the original limits"
    );
    assert!(!canvas.can_undo(), "there was only the one step");
}

// Why: the host draws the band of a Zoom-tool drag itself, so it needs the band in its own coordinates, which must be
// the state's band (clamped to the plot, in figure space) mapped corner for corner through the fit; a band drawn in
// figure points, or through a stale fit, would not lie under the pointer. Twin: none in `app.rs`; the band's
// figure-space geometry is proved in `interaction.rs`.
#[test]
fn the_rubber_band_is_reported_in_screen_points() {
    let (mut canvas, fit) = fitted(figure_with(vec![axes_2d(2)], vec![]));
    let hit = hit_map_of(&mut canvas);
    let (x_map, _) = maps_of(&hit);
    let a = plot_point(&hit, &fit, 0.2, 0.7);
    let b = plot_point(&hit, &fit, 0.6, 0.3);
    assert!(
        canvas.rubber_band(&TEXT).is_none(),
        "there is no band before a drag"
    );

    canvas.set_tool(Tool::Zoom);
    canvas.drag_start(a, &TEXT);
    assert!(
        !canvas.drag_move(b),
        "the Zoom tool moves its band, not the figure"
    );
    let band = canvas
        .state()
        .rubber_band()
        .expect("a Zoom-tool drag on a 2D axes has a band");
    let expected = Rect::from_min_max(
        fit.to_screen.apply(Point::new(band.x, band.y)),
        fit.to_screen.apply(Point::new(band.right(), band.bottom())),
    );
    let reported = canvas
        .rubber_band(&TEXT)
        .expect("the canvas reports the band while dragging");
    assert_pos_close(
        reported.min,
        expected.min,
        1e-3,
        "the band's top-left corner is the state's through the fit",
    );
    assert_pos_close(
        reported.max,
        expected.max,
        1e-3,
        "the band's bottom-right corner is the state's through the fit",
    );
    assert_pos_close(reported.min, a.min(b), 1e-3, "the band spans the drag");
    assert_pos_close(reported.max, a.max(b), 1e-3, "the band spans the drag");

    assert!(
        canvas.drag_end(Some(b)),
        "releasing zooms the axes to the band"
    );
    let x = manual_of(canvas.state().figure(), 2, Dimension::X);
    let (u, v) = (
        x_map.to_data(fit.to_screen.invert(a).x),
        x_map.to_data(fit.to_screen.invert(b).x),
    );
    assert_close(
        x.0,
        u.min(v),
        1e-9,
        "the x limits are the band's data range",
    );
    assert_close(
        x.1,
        u.max(v),
        1e-9,
        "the x limits are the band's data range",
    );
    assert!(
        canvas.rubber_band(&TEXT).is_none(),
        "the band goes with the drag"
    );

    canvas.set_tool(Tool::Pan);
    canvas.drag_start(a, &TEXT);
    canvas.drag_move(b);
    assert!(
        canvas.rubber_band(&TEXT).is_none(),
        "a Pan-tool drag has no band"
    );
    canvas.drag_end(Some(b));
}

// Why: the datatip is tested in figure space, so the controller adds the conversion of the pointer through the fit
// and the choice of mark: a point is ringed where it was drawn, a pixel is outlined so that the reader sees the
// extent that was read, unless it is smaller on screen than the ring, when it is ringed instead so that the mark is
// never too small to see; and nothing is shown while dragging, with no pointer, or where the figure draws nothing,
// so that a callout never follows a pan around. Twins: `hovering_over_a_dense_series_reads_the_point_under_the_pointer`
// and `hovering_over_an_image_reads_the_pixel_under_the_pointer` in `app.rs`.
#[test]
fn a_callout_rings_a_point_and_outlines_a_large_pixel_but_rings_a_small_one() {
    // A point of a scatter is ringed where it was drawn and named with its index and values.
    let (mut canvas, fit) = fitted(figure_with_dense_scatter(2_000));
    let hit = hit_map_of(&mut canvas);
    let plot = hit.axes[0].plot_rect;
    let centre = Point::new(plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);
    // The drawn point nearest the centre of the plot, as its index in the user's arrays and its position.
    let distance = |p: Point| (p.x - centre.x).powi(2) + (p.y - centre.y).powi(2);
    let (source_index, position) = hit.artists[0]
        .samples
        .iter()
        .map(|sample| (sample.source_index, sample.position))
        .min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)))
        .expect("the scatter drew points");
    let pos = fit.to_screen.apply(position);
    let Some(Tip::Point(tip)) = canvas.state().tip_at(&hit, fit.to_screen.invert(pos)) else {
        panic!("the state reads a point where one was drawn");
    };
    canvas.hover(Some(pos));
    let Callout { marker, text } = canvas
        .callout(&TEXT)
        .expect("hovering over a drawn point gives a callout");
    match marker {
        Marker::Ring { centre, radius } => {
            assert_eq!(radius, DATATIP_RING_POINTS, "a point takes the ring");
            assert_pos_close(
                centre,
                fit.to_screen.apply(tip.position),
                1e-3,
                "the ring is centred where the point was drawn",
            );
        }
        other => panic!("a point is ringed, not {other:?}"),
    }
    assert_eq!(text, datatip_text(&tip), "the text is the point datatip's");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("Samples"),
        "the series comes first: {text:?}"
    );
    assert!(
        lines.contains(&format!("index {}", tip.index).as_str()),
        "the text names the index of the point: {text:?}"
    );
    assert_eq!(
        tip.index, source_index,
        "which is the index in the user's array"
    );
    for (label, value) in [("x = ", tip.x), ("y = ", tip.y)] {
        let written: f64 = lines
            .iter()
            .find_map(|line| line.strip_prefix(label))
            .unwrap_or_else(|| panic!("the text has a {label:?} line: {text:?}"))
            .parse()
            .expect("the value is a number");
        assert_close(written, value, 1e-6, "the text gives the point's own value");
    }

    // The callout is withheld while dragging, without a pointer, and outside the figure.
    canvas.drag_start(pos, &TEXT);
    canvas.drag_move(pos + vec2(5.0, 0.0));
    canvas.hover(Some(pos + vec2(5.0, 0.0)));
    assert!(
        canvas.callout(&TEXT).is_none(),
        "no callout while a drag is in progress"
    );
    canvas.drag_end(Some(pos + vec2(5.0, 0.0)));
    canvas.hover(None);
    assert!(
        canvas.callout(&TEXT).is_none(),
        "no callout without a pointer"
    );
    canvas.hover(Some(area().min + vec2(1.0, 1.0)));
    assert!(
        canvas.callout(&TEXT).is_none(),
        "no callout where the figure draws nothing"
    );

    // A pixel large on screen is outlined at its four corners and named with its row, column and value.
    let (mut canvas, fit) = fitted(figure_with_named_image(4, 4));
    let hit = hit_map_of(&mut canvas);
    let pos = plot_point(&hit, &fit, 0.4, 0.6);
    let at = fit.to_screen.invert(pos);
    let (image, row, column) = hit.pixel_at(at).expect("the image fills the axes");
    let Some(Tip::Pixel(tip)) = canvas.state().tip_at(&hit, at) else {
        panic!("the state reads a pixel where no point is drawn");
    };
    canvas.hover(Some(pos));
    let Callout { marker, text } = canvas
        .callout(&TEXT)
        .expect("hovering over an image gives a callout");
    let to_figure = image.to_pixel.inverse().expect("the placement inverts");
    let (c, r) = (column as f64, row as f64);
    let expected = [(c, r), (c + 1.0, r), (c + 1.0, r + 1.0), (c, r + 1.0)]
        .map(|(x, y)| fit.to_screen.apply(to_figure.apply(Point::new(x, y))));
    let bounds = Rect::from_points(&expected);
    assert!(
        bounds.width() >= 2.0 * DATATIP_RING_POINTS && bounds.height() >= 2.0 * DATATIP_RING_POINTS,
        "precondition: a pixel of a 4 × 4 image is larger on screen than the ring: {bounds:?}"
    );
    match marker {
        Marker::Outline(corners) => {
            for (got, want) in corners.iter().zip(expected) {
                assert_pos_close(
                    *got,
                    want,
                    1e-3,
                    "an outline corner is the pixel's corner through the fit",
                );
            }
        }
        other => panic!("a large pixel is outlined, not {other:?}"),
    }
    assert_eq!(
        text,
        pixel_datatip_text(&tip),
        "the text is the pixel datatip's"
    );
    assert!(
        text.contains("Temperature") && text.contains(&format!("row {row}, column {column}")),
        "the text names the image and the pixel: {text:?}"
    );

    // A pixel smaller on screen than the ring is ringed at its centre instead.
    let (mut canvas, fit) = fitted(figure_with_named_image(100, 100));
    let hit = hit_map_of(&mut canvas);
    let pos = plot_point(&hit, &fit, 0.4, 0.6);
    let at = fit.to_screen.invert(pos);
    let (image, row, column) = hit.pixel_at(at).expect("the image fills the axes");
    let Some(Tip::Pixel(tip)) = canvas.state().tip_at(&hit, at) else {
        panic!("the state reads a pixel where no point is drawn");
    };
    let to_figure = image.to_pixel.inverse().expect("the placement inverts");
    let (c, r) = (column as f64, row as f64);
    let bounds = Rect::from_points(
        &[(c, r), (c + 1.0, r), (c + 1.0, r + 1.0), (c, r + 1.0)]
            .map(|(x, y)| fit.to_screen.apply(to_figure.apply(Point::new(x, y)))),
    );
    assert!(
        bounds.width() < 2.0 * DATATIP_RING_POINTS || bounds.height() < 2.0 * DATATIP_RING_POINTS,
        "precondition: a pixel of a 100 × 100 image is smaller on screen than the ring: {bounds:?}"
    );
    canvas.hover(Some(pos));
    let Callout { marker, text } = canvas
        .callout(&TEXT)
        .expect("hovering over an image gives a callout");
    match marker {
        Marker::Ring { centre, radius } => {
            assert_eq!(radius, DATATIP_RING_POINTS, "a small pixel takes the ring");
            assert_pos_close(
                centre,
                fit.to_screen.apply(tip.position),
                1e-3,
                "the ring is centred on the pixel's centre",
            );
        }
        other => panic!("a small pixel is ringed, not {other:?}"),
    }
    assert_eq!(text, pixel_datatip_text(&tip));
}

// Why: the cursor is the only sign of which tool is active before the user drags, and it must follow the tool and
// the drag exactly as the egui canvas maps them today, so that the two hosts feel the same: a hand for Pan that closes
// while dragging, a crosshair for Zoom and a move cursor for Rotate, neither of which changes while dragging.
#[test]
fn the_cursor_follows_the_tool_and_the_drag() {
    let (mut canvas, fit) = fitted(figure_with(vec![axes_2d(2)], vec![]));
    let hit = hit_map_of(&mut canvas);
    let inside = plot_point(&hit, &fit, 0.5, 0.5);

    assert_eq!(canvas.cursor(), Cursor::Grab, "Pan is the default tool");
    canvas.set_tool(Tool::Zoom);
    assert_eq!(canvas.cursor(), Cursor::Crosshair);
    canvas.set_tool(Tool::Rotate);
    assert_eq!(canvas.cursor(), Cursor::Move);
    assert!(
        !canvas.has_3d(),
        "the tool is set as asked even where Rotate has nothing to turn, as the toolbar allows"
    );

    canvas.set_tool(Tool::Pan);
    canvas.drag_start(inside, &TEXT);
    assert_eq!(
        canvas.cursor(),
        Cursor::Grabbing,
        "Pan closes the hand while dragging"
    );
    canvas.drag_end(Some(inside));
    assert_eq!(canvas.cursor(), Cursor::Grab, "and opens it again");

    canvas.set_tool(Tool::Zoom);
    canvas.drag_start(inside, &TEXT);
    assert_eq!(
        canvas.cursor(),
        Cursor::Crosshair,
        "the crosshair does not change while dragging"
    );
    canvas.drag_end(Some(inside));
}

// Why: the two zoom inputs a host has — a wheel in points and a pinch as a ratio — must be combined the way the egui
// canvas combines them, so that one notch of a typical wheel (50 points) zooms by about a fifth and a pinch scales
// directly; a host that got the formula slightly wrong would zoom at a different rate from the other host.
#[test]
fn wheel_factor_is_exponential_in_the_scroll_and_multiplies_the_pinch() {
    assert_close(
        wheel_factor(0.0, 1.0),
        1.0,
        1e-12,
        "no scroll and no pinch is no zoom",
    );
    assert_close(
        wheel_factor(50.0, 1.0),
        (50.0 * WHEEL_ZOOM_RATE).exp(),
        1e-12,
        "a scroll zooms by exp(points × rate)",
    );
    assert!(
        (wheel_factor(50.0, 1.0) - 1.2).abs() < 0.01,
        "one notch of a typical wheel zooms by about 20 %: {}",
        wheel_factor(50.0, 1.0)
    );
    assert_close(
        wheel_factor(-50.0, 1.0) * wheel_factor(50.0, 1.0),
        1.0,
        1e-12,
        "scrolling back undoes the zoom",
    );
    assert_close(
        wheel_factor(0.0, 2.0),
        2.0,
        1e-12,
        "a pinch is the factor itself",
    );
    assert_close(
        wheel_factor(50.0, 2.0),
        2.0 * (50.0 * WHEEL_ZOOM_RATE).exp(),
        1e-12,
        "both together multiply",
    );
}

// Why: the return values of undo, redo and reset are what tell a host whether to recompile and redraw; a host that
// redrew on a `false` would waste a frame and one that skipped a `true` would show a stale figure, so each must say
// exactly whether the figure changed, both when there is history to move through and when there is none.
#[test]
fn undo_redo_and_reset_view_report_whether_anything_changed() {
    let (mut canvas, fit) = fitted(figure_with(vec![axes_2d(2)], vec![]));
    assert!(!canvas.undo(), "nothing to undo on a fresh canvas");
    assert!(!canvas.redo(), "nothing to redo on a fresh canvas");
    assert!(!canvas.reset_view(), "nothing to reset on a fresh canvas");
    assert!(!canvas.can_undo() && !canvas.can_redo());

    pan_right(&mut canvas, &fit, 60.0);
    let panned = manual_of(canvas.state().figure(), 2, Dimension::X);
    assert_ne!(panned, (0.0, 10.0), "precondition: the pan moved the view");

    assert!(canvas.undo(), "the pan is undone");
    assert_eq!(
        manual_of(canvas.state().figure(), 2, Dimension::X),
        (0.0, 10.0)
    );
    assert!(canvas.can_redo());
    assert!(canvas.redo(), "and redone");
    assert_eq!(manual_of(canvas.state().figure(), 2, Dimension::X), panned);
    assert!(canvas.reset_view(), "the reset restores the source's view");
    assert_eq!(
        manual_of(canvas.state().figure(), 2, Dimension::X),
        (0.0, 10.0)
    );
    assert!(
        !canvas.reset_view(),
        "a second reset has nothing left to restore"
    );
    assert!(
        canvas.can_undo(),
        "the reset is a step of the history, as the toolbar's Refit is"
    );
}

// Why: the toolbar shows one list of what is wrong with the figure, gathered as `FigurePane::ui` gathers it today —
// the scene compiler's warnings, in its order, then the problems of the user's own changes — so that the reader sees
// what the figure cannot draw before what their edits could not do; a controller that dropped either source, or
// interleaved them, would change what the indicator counts and lists. Twin: `a_dropped_overlay_entry_is_reported_by_
// the_problems_indicator` in `app.rs` for the state's half, and the scene compiler's own tests for its warnings.
#[test]
fn problems_lists_the_scene_warnings_before_the_state_problems() {
    let (mut canvas, _) = fitted(figure_with_empty_line());
    let problems = canvas.problems(&TEXT);
    assert_eq!(
        problems.len(),
        1,
        "an empty line raises one warning and nothing else: {problems:?}"
    );
    assert_eq!(problems[0].origin, Origin::Scene);
    assert_eq!(
        problems[0].node,
        Some(NodeId(3)),
        "the warning names the line"
    );
    assert!(
        problems[0].detail.contains("no points"),
        "and says why it is not drawn: {:?}",
        problems[0].detail
    );

    // Limits that are not increasing are dropped when the overlay is composed, which the state reports.
    canvas
        .state_mut()
        .record(&set(2, "x.limits", Value::Limits(manual(4.0, 4.0))));
    canvas.invalidate();
    let problems = canvas.problems(&TEXT);
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert_eq!(
        problems[0].origin,
        Origin::Scene,
        "the scene's warning comes first"
    );
    assert_eq!(
        problems[1].origin,
        Origin::Discarded,
        "then the discarded change"
    );
    assert_eq!(
        problems[1].node,
        Some(NodeId(2)),
        "which names the axes it concerned"
    );
    assert_eq!(
        problems[1].path.as_ref().map(ToString::to_string),
        Some("x.limits".to_owned()),
        "and the property"
    );

    // Discarding every change, as "Revert all changes" does, takes the state's problem with it.
    canvas.state_mut().revert_all();
    canvas.invalidate();
    let problems = canvas.problems(&TEXT);
    assert_eq!(
        problems.iter().map(|p| p.origin).collect::<Vec<_>>(),
        vec![Origin::Scene],
        "the state's problem goes with its change, while the scene's warning stays as long as the figure is drawn"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// The pointer recogniser
// ---------------------------------------------------------------------------------------------------------------------

const ORIGIN: Pos2 = pos2(100.0, 100.0);

// Why: the egui host acts on the click egui decides, and a browser host must decide the same way from raw events; a
// press released without moving far or lasting long is a click, reported once, at the release position, and a press
// alone reports nothing because nothing is decided until it ends.
#[test]
fn a_press_released_within_the_click_distance_and_duration_is_a_click() {
    let mut pointer = Pointer::default();
    assert!(
        pointer.down(ORIGIN, 0.0).is_empty(),
        "a press decides nothing by itself"
    );
    let released = ORIGIN + vec2(2.0, 1.0);
    assert_eq!(
        pointer.up(Some(released), 0.1),
        vec![Gesture::Click(released)],
        "the click is where the button was released"
    );
}

// Why: a second click soon after the first is a double click and nothing else — egui reports the second press as a
// click of count two, and the figure canvas acts on the double click alone, so a recogniser that also reported a
// click would toggle a legend entry twice — and a click after the delay has passed is a plain click again.
#[test]
fn two_clicks_within_the_double_click_delay_are_a_double_click_and_a_later_one_is_a_click() {
    let mut pointer = Pointer::default();
    pointer.down(ORIGIN, 0.0);
    assert_eq!(pointer.up(Some(ORIGIN), 0.05), vec![Gesture::Click(ORIGIN)]);

    let second = 0.05 + Pointer::MAX_DOUBLE_CLICK_DELAY / 2.0;
    pointer.down(ORIGIN, second);
    assert_eq!(
        pointer.up(Some(ORIGIN), second + 0.05),
        vec![Gesture::DoubleClick(ORIGIN)],
        "the second click is a double click and not also a click"
    );

    let third = second + 0.05 + Pointer::MAX_DOUBLE_CLICK_DELAY * 2.0;
    pointer.down(ORIGIN, third);
    assert_eq!(
        pointer.up(Some(ORIGIN), third + 0.05),
        vec![Gesture::Click(ORIGIN)],
        "a click beyond the delay starts afresh"
    );
}

// Why: a press that moves beyond egui's click distance is a drag, and the figure canvas must be told where it began —
// the press origin, not where the pointer was when the distance was crossed — so that a pan anchors the data point
// first grabbed; every move after that is a drag move, and the release ends the drag without also clicking, or a
// pan would end by selecting whatever lay under the pointer.
#[test]
fn a_press_moved_beyond_the_click_distance_is_a_drag_from_the_press_origin() {
    let mut pointer = Pointer::default();
    pointer.down(ORIGIN, 0.0);
    let nearby = ORIGIN + vec2(Pointer::MAX_CLICK_DISTANCE - 1.0, 0.0);
    assert!(
        pointer.moved(nearby, 0.05).is_empty(),
        "a move within the click distance decides nothing yet"
    );
    let far = ORIGIN + vec2(60.0, 0.0);
    assert_eq!(
        pointer.moved(far, 0.1),
        vec![Gesture::DragStart(ORIGIN), Gesture::DragMove(far)],
        "crossing the distance starts the drag at the press and moves it to the pointer"
    );
    let further = far + vec2(10.0, 10.0);
    assert_eq!(
        pointer.moved(further, 0.15),
        vec![Gesture::DragMove(further)]
    );
    assert_eq!(
        pointer.up(Some(further), 0.2),
        vec![Gesture::DragEnd(Some(further))],
        "the release ends the drag and is not a click"
    );
}

// Why: egui also treats a press held longer than its click duration as a drag, however little it moved, so that a
// slow pan that begins with a pause does not end as a click; a recogniser that decided by distance alone would
// diverge from the egui host on exactly such a gesture.
#[test]
fn a_press_held_beyond_the_click_duration_is_a_drag_however_little_it_moved() {
    let mut pointer = Pointer::default();
    pointer.down(ORIGIN, 0.0);
    let barely = ORIGIN + vec2(1.0, 0.0);
    assert_eq!(
        pointer.moved(barely, Pointer::MAX_CLICK_DURATION + 0.2),
        vec![Gesture::DragStart(ORIGIN), Gesture::DragMove(barely)],
        "the first move after the duration has passed starts the drag"
    );
    assert_eq!(
        pointer.up(Some(barely), Pointer::MAX_CLICK_DURATION + 0.3),
        vec![Gesture::DragEnd(Some(barely))]
    );

    let mut pointer = Pointer::default();
    pointer.down(ORIGIN, 0.0);
    assert_eq!(
        pointer.up(Some(ORIGIN), Pointer::MAX_CLICK_DURATION / 2.0),
        vec![Gesture::Click(ORIGIN)],
        "a press released within the duration, without a move, is still a click"
    );
}

// Why: a host may lose the pointer while a button is down — it left the window, or a touch was cancelled — and the
// egui canvas then ends the drag with no position; the recogniser must report the same so that the figure canvas
// closes the drag rather than leaving the state mid-gesture.
#[test]
fn a_release_without_a_position_while_dragging_ends_the_drag_with_none() {
    let mut pointer = Pointer::default();
    pointer.down(ORIGIN, 0.0);
    pointer.moved(ORIGIN + vec2(60.0, 0.0), 0.1);
    assert_eq!(pointer.up(None, 0.2), vec![Gesture::DragEnd(None)]);
    assert!(
        pointer.moved(ORIGIN, 0.3).is_empty(),
        "nothing is dragging afterwards"
    );
}

// Why: a hover is not a gesture, and a release that no press preceded is noise from a button pressed elsewhere; the
// recogniser must report nothing for either, or the figure canvas would see drags and clicks the user never made.
#[test]
fn a_move_with_no_button_down_and_a_release_with_no_press_decide_nothing() {
    let mut pointer = Pointer::default();
    assert!(pointer.moved(ORIGIN, 0.0).is_empty());
    assert!(pointer.moved(ORIGIN + vec2(200.0, 0.0), 0.1).is_empty());
    assert!(
        pointer.up(Some(ORIGIN), 0.2).is_empty(),
        "a release with no press is not a click"
    );
}

// Why: releasing a drag is not a click, so it must not count towards a double click either: a quick click after a
// drag is a plain click, as it is in egui, where only a release that registered as a click records the click time.
#[test]
fn a_click_soon_after_a_drag_is_a_click_and_not_a_double_click() {
    let mut pointer = Pointer::default();
    pointer.down(ORIGIN, 0.0);
    pointer.moved(ORIGIN + vec2(60.0, 0.0), 0.1);
    pointer.up(Some(ORIGIN + vec2(60.0, 0.0)), 0.2);

    pointer.down(ORIGIN + vec2(60.0, 0.0), 0.25);
    assert_eq!(
        pointer.up(Some(ORIGIN + vec2(60.0, 0.0)), 0.3),
        vec![Gesture::Click(ORIGIN + vec2(60.0, 0.0))]
    );
}
