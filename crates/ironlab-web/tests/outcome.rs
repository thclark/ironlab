//! Native tests of the plain Rust side of `ironlab-web`: the [`Outcome`] a page reads after every input, built from a
//! [`FigureCanvas`] exactly as the handle builds it, and the stem a download is named by. Both are ordinary Rust in
//! the `rlib`, so they are proved with `cargo test -p ironlab-web` on the host, without a browser, and the browser
//! tests in `web.rs` only have to show that the `wasm_bindgen` surface hands the same values to JavaScript.
//!
//! The canvas is fitted into a 400 × 300 area, the size the browser tests give their canvas, so that the two suites
//! speak of the same geometry.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::sync::LazyLock;

use emath::{Pos2, Rect, pos2, vec2};
use ironlab_canvas::{DATATIP_RING_POINTS, FigureCanvas, MAX_TILE_SIDE, Tool};
use ironlab_ir::{
    Artist, Axes, Axis, DataId, Figure, FigureSize, Limits, Line, NdArray, NodeId, Text,
};
use ironlab_scene::display::Point;
use ironlab_text::TextEngine;
use ironlab_web::{Datatip, Marker, Outcome, download_stem};
use serde_json::json;

/// One text engine for the whole test binary; building it parses the bundled fonts.
static TEXT: LazyLock<TextEngine> = LazyLock::new(TextEngine::new);

/// The area the browser tests give their canvas, in CSS pixels, which are the canvas's screen points.
fn area() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0))
}

/// A figure of one two-dimensional axes with manual limits x ∈ [0, 10] and y ∈ [0, 5], holding one line through
/// three points, so that there is an axes to pan and zoom and a drawn point to hover.
fn figure() -> Figure {
    let (x, y) = (DataId(10), DataId(11));
    Figure {
        id: NodeId(1),
        data: BTreeMap::from([
            (x, NdArray::vector(vec![2.0, 5.0, 8.0])),
            (y, NdArray::vector(vec![1.0, 2.5, 4.0])),
        ]),
        axes: vec![Axes {
            id: NodeId(2),
            x: Axis {
                limits: Limits::Manual {
                    min: 0.0,
                    max: 10.0,
                },
                ..Axis::default()
            },
            y: Axis {
                limits: Limits::Manual { min: 0.0, max: 5.0 },
                ..Axis::default()
            },
            artists: vec![Artist::Line(Line {
                id: NodeId(3),
                display_name: Some(Text::plain("Samples")),
                x,
                y,
                ..Line::default()
            })],
            ..Axes::default()
        }],
        ..Figure::new()
    }
}

/// A canvas of `figure` fitted into [`area`].
fn fitted(figure: Figure) -> FigureCanvas {
    let mut canvas = FigureCanvas::new(figure);
    canvas.resize(area(), MAX_TILE_SIDE);
    canvas
        .fit(&TEXT)
        .expect("a figure with a page fits into a 400 × 300 area");
    canvas
}

/// The screen position of the centre of the plot rectangle of the first axes.
fn plot_centre(canvas: &mut FigureCanvas) -> Pos2 {
    let plot = canvas.scene(&TEXT).hit_map.axes[0].plot_rect;
    let centre = Point::new(plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);
    let fit = canvas.fit(&TEXT).expect("the canvas is fitted");
    fit.to_screen.apply(centre)
}

/// The screen position at which the point with `source_index` in the user's arrays was drawn.
fn drawn_point(canvas: &mut FigureCanvas, source_index: usize) -> Pos2 {
    let position = canvas
        .scene(&TEXT)
        .hit_map
        .artists
        .iter()
        .flat_map(|artist| &artist.samples)
        .find(|sample| sample.source_index == source_index)
        .map(|sample| sample.position)
        .unwrap_or_else(|| panic!("the line drew its point {source_index}"));
    let fit = canvas.fit(&TEXT).expect("the canvas is fitted");
    fit.to_screen.apply(position)
}

/// The outcome of the canvas as it stands, with `redraw` as the gesture reported it.
fn outcome_of(canvas: &mut FigureCanvas, redraw: bool) -> Outcome {
    Outcome::from_canvas(canvas, &TEXT, redraw)
}

#[track_caller]
fn assert_close(actual: f32, expected: f32, tolerance: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected}, got {actual} (tolerance {tolerance})"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// The download stem
// ---------------------------------------------------------------------------------------------------------------------

// Why: a page names the download after the file the user opened, and the user expects `surf.fig.json` to come back
// as `surf.pdf` or `surf.fig.json`, never `surf.fig.pdf`; the name may carry a directory when it came from a path and
// may be spelt in any case, as `Format::from_name` accepts, and a name with no stem at all must still give the
// download a name.
#[test]
fn download_stem_strips_the_figure_extension_and_any_directory() {
    let cases = [
        ("figures/surf.fig.json", "surf"),
        ("surf.fig.json", "surf"),
        ("surf.fig", "surf"),
        ("surf.json", "surf"),
        ("SURF.FIG", "SURF"),
        ("Surf.Fig.JSON", "Surf"),
        ("surf", "surf"),
        ("nested/dir/model.v2.fig", "model.v2"),
        ("results/run 7.fig", "run 7"),
        ("", "figure"),
        ("figures/", "figure"),
        (".fig", "figure"),
    ];
    for (name, stem) in cases {
        assert_eq!(download_stem(name), stem, "the stem of {name:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The outcome of an input
// ---------------------------------------------------------------------------------------------------------------------

// Why: before any input the page must show an open hand and nothing else, with nothing to undo or redo and no problem
// to indicate, and `redraw` must be what the gesture reported rather than anything the outcome infers, or a page
// would redraw on every hover.
#[test]
fn an_outcome_at_rest_reports_the_open_hand_and_nothing_else() {
    let mut canvas = fitted(figure());
    assert_eq!(
        outcome_of(&mut canvas, false),
        Outcome {
            redraw: false,
            cursor: "grab",
            rubber_band: None,
            datatip: None,
            can_undo: false,
            can_redo: false,
            problem_count: 0,
        }
    );
    assert!(
        outcome_of(&mut canvas, true).redraw,
        "redraw is the gesture's own report, passed through"
    );
}

// Why: the cursor is the only sign of the active tool before the user drags, so it must name the tool exactly as CSS
// spells it, and close into a grabbing hand for as long as a pan is in progress; the undo state must follow the drag
// so that the toolbar enables Undo as soon as there is something to undo.
#[test]
fn the_cursor_follows_the_tool_and_the_drag_and_undo_follows_the_change() {
    let mut canvas = fitted(figure());
    canvas.set_tool(Tool::Zoom);
    assert_eq!(outcome_of(&mut canvas, false).cursor, "crosshair");
    canvas.set_tool(Tool::Rotate);
    assert_eq!(outcome_of(&mut canvas, false).cursor, "move");
    canvas.set_tool(Tool::Pan);
    assert_eq!(outcome_of(&mut canvas, false).cursor, "grab");

    let from = plot_centre(&mut canvas);
    canvas.drag_start(from, &TEXT);
    let started = outcome_of(&mut canvas, false);
    assert_eq!(
        started.cursor, "grabbing",
        "the hand closes as soon as the drag starts"
    );
    assert!(!started.can_undo, "starting a drag changes nothing yet");

    let changed = canvas.drag_move(from + vec2(40.0, 0.0));
    assert!(changed, "a pan across the plot changes the limits");
    let moved = outcome_of(&mut canvas, changed);
    assert!(moved.redraw);
    assert_eq!(moved.cursor, "grabbing");
    assert!(
        !moved.can_undo,
        "the drag is one undo step, held open until it ends, so nothing is undoable yet"
    );
    assert!(!moved.can_redo);
    assert!(moved.rubber_band.is_none(), "Pan has no rubber band");
    assert!(
        moved.datatip.is_none(),
        "nothing is read under the pointer while dragging"
    );

    let ended = canvas.drag_end(Some(from + vec2(40.0, 0.0)));
    let released = outcome_of(&mut canvas, ended);
    assert_eq!(
        released.cursor, "grab",
        "the hand opens again when the drag ends"
    );
    assert!(
        released.can_undo && !released.can_redo,
        "the whole drag is undoable as one step once it has ended"
    );

    assert!(canvas.undo(), "there is a pan to undo");
    let undone = outcome_of(&mut canvas, true);
    assert!(!undone.can_undo && undone.can_redo);
    assert_eq!(undone.cursor, "grab");
}

// Why: a page draws the rubber band of a Zoom-tool drag itself, so the outcome must give it as `[x, y, width, height]`
// in CSS pixels with a non-negative size whichever way the user dragged, and drop it the moment the drag ends.
#[test]
fn a_zoom_drag_reports_its_rubber_band_as_x_y_width_and_height_in_screen_points() {
    for (dx, dy) in [(40.0_f32, 30.0_f32), (-40.0, -30.0)] {
        let mut canvas = fitted(figure());
        canvas.set_tool(Tool::Zoom);
        let from = plot_centre(&mut canvas);
        let to = from + vec2(dx, dy);
        canvas.drag_start(from, &TEXT);
        let changed = canvas.drag_move(to);
        assert!(
            !changed,
            "a Zoom-tool drag moves its band without changing the figure"
        );

        let outcome = outcome_of(&mut canvas, changed);
        assert_eq!(outcome.cursor, "crosshair");
        let [x, y, width, height] = outcome
            .rubber_band
            .unwrap_or_else(|| panic!("a Zoom drag of ({dx}, {dy}) has a rubber band"));
        let tolerance = 0.05;
        assert_close(
            x,
            from.x.min(to.x),
            tolerance,
            "the band starts at the left edge of the drag",
        );
        assert_close(y, from.y.min(to.y), tolerance, "and at its top edge");
        assert_close(
            width,
            dx.abs(),
            tolerance,
            "its width is the horizontal extent of the drag",
        );
        assert_close(
            height,
            dy.abs(),
            tolerance,
            "its height is the vertical extent",
        );

        canvas.drag_end(Some(to));
        assert!(
            outcome_of(&mut canvas, true).rubber_band.is_none(),
            "the band is gone once the drag has ended"
        );
    }
}

// Why: hovering a drawn point shows the user its values and its index in their own data, so the outcome must carry
// the callout's text and ring it where the point was drawn, with the anchor a page positions its tooltip by at the
// ring's centre; and nothing must be reported once the pointer has left the canvas.
#[test]
fn hovering_a_drawn_point_gives_a_datatip_ringed_and_anchored_where_it_was_drawn() {
    let mut canvas = fitted(figure());
    let at = drawn_point(&mut canvas, 1);
    canvas.hover(Some(at));
    let Datatip {
        text,
        anchor,
        marker,
    } = outcome_of(&mut canvas, false)
        .datatip
        .expect("hovering over a drawn point gives a datatip");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("Samples"),
        "the series comes first: {text:?}"
    );
    assert!(
        lines.contains(&"x = 5"),
        "the text names the x value: {text:?}"
    );
    assert!(lines.contains(&"y = 2.5"), "and the y value: {text:?}");
    assert!(
        lines.contains(&"index 1"),
        "and the index in the user's arrays: {text:?}"
    );
    let Marker::Ring { cx, cy, r } = marker else {
        panic!("a point is ringed, not {marker:?}");
    };
    assert_eq!(r, DATATIP_RING_POINTS, "the ring is the canvas's own size");
    assert_close(
        cx,
        at.x,
        1e-3,
        "the ring is centred where the point was drawn",
    );
    assert_close(
        cy,
        at.y,
        1e-3,
        "the ring is centred where the point was drawn",
    );
    assert_eq!(
        anchor,
        [cx, cy],
        "the tooltip is anchored at the ring's centre"
    );

    canvas.hover(None);
    assert!(
        outcome_of(&mut canvas, false).datatip.is_none(),
        "nothing is read once the pointer has left"
    );
}

// Why: the problems indicator of a page shows a count and opens the list on demand, so the outcome must count what
// the canvas reports, warnings of the scene included, and agree with the list the page would fetch.
#[test]
fn problem_count_counts_what_the_canvas_reports() {
    let mut sizeless = figure();
    sizeless.size = FigureSize {
        width_mm: 0.0,
        height_mm: 0.0,
    };
    let mut canvas = fitted(sizeless);
    let problems = canvas.problems(&TEXT);
    assert!(
        problems.iter().any(|p| p.detail.contains("not usable")),
        "a figure of no size is drawn at the default size with a warning: {problems:?}"
    );
    assert_eq!(outcome_of(&mut canvas, false).problem_count, problems.len());
}

// ---------------------------------------------------------------------------------------------------------------------
// The JavaScript contract
// ---------------------------------------------------------------------------------------------------------------------

// Why: the page reads `marker.kind` to choose between drawing a ring and a polygon, so the tag, its spelling and the
// field names are the contract with the JavaScript; serialising with serde_json pins the same shape
// serde-wasm-bindgen produces, without a browser.
#[test]
fn markers_serialise_to_the_tagged_shape_the_page_reads() {
    let ring = Marker::Ring {
        cx: 1.5,
        cy: 2.0,
        r: 4.0,
    };
    assert_eq!(
        serde_json::to_string(&ring).expect("a marker serialises"),
        r#"{"kind":"ring","cx":1.5,"cy":2.0,"r":4.0}"#
    );
    let outline = Marker::Outline {
        points: [[0.0, 0.0], [10.0, 0.0], [10.0, 6.0], [0.0, 6.0]],
    };
    assert_eq!(
        serde_json::to_value(&outline).expect("a marker serialises"),
        json!({
            "kind": "outline",
            "points": [[0.0, 0.0], [10.0, 0.0], [10.0, 6.0], [0.0, 6.0]],
        })
    );
}

// Why: a page destructures every outcome by name, so the object must have exactly the documented keys, with the
// optional ones present as `null` rather than missing, and the cursor as the CSS keyword.
#[test]
fn an_outcome_serialises_with_exactly_the_documented_keys() {
    let mut canvas = fitted(figure());
    let value =
        serde_json::to_value(outcome_of(&mut canvas, false)).expect("an outcome serialises");
    let object = value.as_object().expect("an outcome is an object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "can_redo",
            "can_undo",
            "cursor",
            "datatip",
            "problem_count",
            "redraw",
            "rubber_band",
        ]
    );
    assert_eq!(object["redraw"], json!(false));
    assert_eq!(object["cursor"], json!("grab"));
    assert!(
        object["rubber_band"].is_null(),
        "an absent band is null, not missing"
    );
    assert!(
        object["datatip"].is_null(),
        "an absent datatip is null, not missing"
    );
    assert_eq!(object["problem_count"], json!(0));
}
