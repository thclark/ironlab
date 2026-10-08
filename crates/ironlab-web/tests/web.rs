//! Browser tests of `ironlab-web`, run in headless Chrome by `wasm-bindgen-test-runner`:
//!
//! ```text
//! cargo test --target wasm32-unknown-unknown -p ironlab-web
//! ```
//!
//! The crate is the browser host of IronLAB figures, and these tests drive it exactly as a page would: through the
//! `wasm_bindgen` surface of [`Session`] and [`FigureHandle`], reading each outcome back from the `JsValue` the
//! handle returns rather than from any Rust type behind it. Every test but the last needs no GPU, because headless
//! Chrome in a container often has neither WebGPU nor WebGL2: they open figures through [`Session::headless`], a
//! session with a text engine and no graphics device, whose `open_headless` decodes and drives a figure without a
//! surface. Nothing they prove depends on a renderer, so a machine without one runs them in full. The one test that
//! renders on a `<canvas>` skips itself with a console warning when [`Session::create`] fails, in the spirit of the
//! offscreen tests' `gpu_or_skip`; it is a smoke test, not a gate.
//!
//! The fixtures under `fixtures/` are figures the documentation gallery exported: `line_markers` (a two-dimensional
//! figure of lines, markers and LaTeX labels, in both encodings), `surf` (a three-dimensional surface) and
//! `latex_labels` (a figure whose title, labels and legend are all typeset mathematics).

#![cfg(target_arch = "wasm32")]

use ironlab_canvas::files::Format;
use ironlab_ir::{Axes, Axis, Figure, Limits, NodeId, Text};
use ironlab_text::TextEngine;
use ironlab_web::{FigureHandle, Session};
use js_sys::Uint8Array;
use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;
use web_sys::HtmlCanvasElement;

wasm_bindgen_test_configure!(run_in_browser);

const LINE_MARKERS_FIG: &[u8] = include_bytes!("fixtures/line_markers.fig");
const LINE_MARKERS_JSON: &[u8] = include_bytes!("fixtures/line_markers.fig.json");
const SURF_FIG: &[u8] = include_bytes!("fixtures/surf.fig");
const LATEX_LABELS_FIG: &[u8] = include_bytes!("fixtures/latex_labels.fig");

/// The keys of an outcome, in the order the contract documents them.
const OUTCOME_KEYS: [&str; 7] = [
    "redraw",
    "cursor",
    "rubber_band",
    "datatip",
    "can_undo",
    "can_redo",
    "problem_count",
];

/// One point per millimetre of the figure's size: the gallery figures are 160 × 100 mm.
const PT_PER_MM: f32 = 72.0 / 25.4;

// ---------------------------------------------------------------------------------------------------------------------
// What a page reads back
// ---------------------------------------------------------------------------------------------------------------------

/// The outcome of an input as a page reads it: the `JsValue` deserialised into the documented shape. Unknown fields
/// are refused, so an outcome that grew a field the page was not told about fails here.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Outcome {
    redraw: bool,
    cursor: String,
    rubber_band: Option<[f32; 4]>,
    datatip: Option<Datatip>,
    can_undo: bool,
    can_redo: bool,
    problem_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Datatip {
    text: String,
    anchor: [f32; 2],
    marker: Marker,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
#[allow(dead_code)]
enum Marker {
    Ring { cx: f32, cy: f32, r: f32 },
    Outline { points: [[f32; 2]; 4] },
}

/// One entry of `problems()`, and one warning of an export, as a page reads it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Problem {
    subject: String,
    detail: String,
    explanation: String,
}

/// What `export_pdf()` resolves to, as a page reads it: the bytes of the PDF and the exporter's warnings.
struct Export {
    bytes: Vec<u8>,
    warnings: Vec<Problem>,
}

/// Reads an export back from the value `export_pdf()` resolved to.
fn export(value: JsValue) -> Export {
    assert!(
        value.is_object(),
        "export_pdf resolves to an object: {value:?}"
    );
    let bytes =
        js_sys::Reflect::get(&value, &JsValue::from_str("bytes")).expect("an object answers `get`");
    assert!(
        bytes.is_instance_of::<Uint8Array>(),
        "the export's `bytes` is a Uint8Array: {bytes:?}"
    );
    let warnings = js_sys::Reflect::get(&value, &JsValue::from_str("warnings"))
        .expect("an object answers `get`");
    let warnings: Vec<Problem> = serde_wasm_bindgen::from_value(warnings.clone())
        .unwrap_or_else(|e| panic!("the export's `warnings` is an array of {{subject, detail, explanation}}: {e} in {warnings:?}"));
    Export {
        bytes: Uint8Array::new(&bytes).to_vec(),
        warnings,
    }
}

/// Reads an outcome back from the value an input method returned.
fn outcome(value: JsValue) -> Outcome {
    serde_wasm_bindgen::from_value(value.clone()).unwrap_or_else(|e| {
        panic!("an outcome is a plain object of the documented fields: {e} in {value:?}")
    })
}

/// Reads the problems of a handle back from the array `problems()` returned.
fn problems(handle: &FigureHandle) -> Vec<Problem> {
    let value = handle.problems();
    serde_wasm_bindgen::from_value(value.clone()).unwrap_or_else(|e| {
        panic!("problems() is an array of {{subject, detail, explanation}}: {e} in {value:?}")
    })
}

/// Opens `bytes` in `format` on a headless session, panicking with the error the handle gave.
fn headless(bytes: &[u8], format: &str) -> FigureHandle {
    Session::headless()
        .open_headless(bytes, format)
        .map_err(JsValue::from)
        .unwrap_or_else(|e| panic!("a gallery figure opens headless as {format:?}: {e:?}"))
}

/// Decodes `bytes` with the codec the crate itself uses for `format`.
fn decode(format: Format, bytes: &[u8]) -> Figure {
    format
        .decode(bytes)
        .unwrap_or_else(|e| panic!("the fixture decodes as {format:?}: {e}"))
}

/// The position, in CSS pixels of a 400 × 300 canvas, of the first legend entry of the figure `handle` holds, found
/// by compiling the same figure and fitting it as the handle fits it, so that the test lands a press exactly where
/// the handle will hit-test a legend entry.
fn legend_entry(handle: &FigureHandle) -> emath::Pos2 {
    let figure = decode(Format::Fig, &handle.save());
    let text = TextEngine::new();
    let mut canvas = ironlab_canvas::FigureCanvas::new(figure);
    canvas.resize(
        emath::Rect::from_min_size(emath::Pos2::ZERO, emath::vec2(400.0, 300.0)),
        ironlab_canvas::MAX_TILE_SIDE,
    );
    let fit = canvas
        .fit(&text)
        .expect("the figure fits a 400 × 300 canvas");
    let entry = canvas
        .scene(&text)
        .hit_map
        .legend_entries
        .first()
        .expect("the figure has a legend")
        .rect;
    fit.to_screen.apply(ironlab_scene::display::Point::new(
        entry.x + entry.width / 2.0,
        entry.y + entry.height / 2.0,
    ))
}

/// A `<canvas>` of `width` by `height` device pixels appended to the document body, as a page would create one.
fn canvas_element(width: u32, height: u32) -> HtmlCanvasElement {
    let document = web_sys::window()
        .expect("the tests run in a window")
        .document()
        .expect("the window has a document");
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .expect("a canvas element is created")
        .dyn_into()
        .expect("what was created is a canvas");
    canvas.set_width(width);
    canvas.set_height(height);
    document
        .body()
        .expect("the document has a body")
        .append_child(&canvas)
        .expect("the canvas is appended to the body");
    canvas
}

/// The LaTeX of `depth` fractions nested in one another's denominators, as ironlab-text's own tests build it.
fn nested_fractions(depth: usize) -> String {
    format!("${}x{}$", r"\frac{1}{".repeat(depth), "}".repeat(depth))
}

/// A figure of one two-dimensional axes with manual limits, titled `title`, so that the title is the only text that
/// could possibly fail to typeset.
fn titled_figure(title: &str) -> Figure {
    Figure {
        id: NodeId(1),
        title: Some(Text::new(title)),
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
            ..Axes::default()
        }],
        ..Figure::new()
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Decoding and saving, without a GPU
// ---------------------------------------------------------------------------------------------------------------------

// Why: a page receives bytes and a format name, never a file path, so the host must decode either encoding from
// bytes alone, and the two encodings of one gallery figure must give the same figure or the browser would show a
// different figure depending on which file the user dropped. The wrong codec, or a name that is no format, must be
// an error a page can show rather than a garbled figure.
#[wasm_bindgen_test]
fn a_protobuf_figure_and_its_json_twin_decode_to_the_same_figure() {
    let from_fig = decode(Format::Fig, LINE_MARKERS_FIG);
    let from_json = decode(Format::Json, LINE_MARKERS_JSON);
    assert_eq!(
        from_fig, from_json,
        "the gallery wrote both files from one figure, and both decode back to it"
    );

    let session = Session::headless();
    let fig = session
        .open_headless(LINE_MARKERS_FIG, "fig")
        .map_err(JsValue::from)
        .expect("the protobuf bytes open as \"fig\"");
    let json = session
        .open_headless(LINE_MARKERS_JSON, "json")
        .map_err(JsValue::from)
        .expect("the JSON bytes open as \"json\"");
    assert_eq!(fig.format(), "fig");
    assert_eq!(json.format(), "json");
    assert_eq!(
        fig.size_pt(),
        json.size_pt(),
        "the handles hold the same figure, so they agree on its size"
    );
    assert_eq!(fig.has_3d(), json.has_3d());
    assert_eq!(
        decode(Format::Fig, &fig.save()),
        decode(Format::Json, &json.save()),
        "and on what they save"
    );

    assert!(
        session.open_headless(LINE_MARKERS_JSON, "fig").is_err(),
        "JSON bytes are not a protobuf figure"
    );
    assert!(
        session.open_headless(LINE_MARKERS_FIG, "json").is_err(),
        "protobuf bytes are not a JSON figure"
    );
    assert!(
        session.open_headless(LINE_MARKERS_FIG, "svg").is_err(),
        "a name that is no figure format is refused"
    );
    assert!(
        session.open_headless(&[], "fig").is_err(),
        "no bytes are no figure"
    );
}

// Why: the toolbar of a page enables and labels itself from what the handle reports before any input, so a handle
// must describe the figure it holds correctly on opening: whether Rotate has anything to turn, the page size the
// page lays its canvas out from, the tool a drag would use, and that there is nothing yet to undo.
#[wasm_bindgen_test]
fn a_handle_reports_the_figure_it_holds() {
    let flat = headless(LINE_MARKERS_FIG, "fig");
    assert!(!flat.has_3d(), "a figure of lines has nothing to rotate");
    let size = flat.size_pt();
    assert_eq!(
        size.len(),
        2,
        "size_pt() is [width_pt, height_pt]: {size:?}"
    );
    let expected = [160.0 * PT_PER_MM, 100.0 * PT_PER_MM];
    for (axis, (got, want)) in size.iter().zip(expected).enumerate() {
        assert!(
            (got - want).abs() < 0.01,
            "size_pt()[{axis}] is {want} for a 160 × 100 mm figure, not {got}"
        );
    }
    assert_eq!(flat.tool(), "pan", "Pan is the default tool");
    assert!(!flat.can_undo());
    assert!(!flat.can_redo());
    assert!(
        problems(&flat).is_empty(),
        "a gallery figure draws cleanly: {:?}",
        problems(&flat)
    );

    let surface = headless(SURF_FIG, "fig");
    assert!(
        surface.has_3d(),
        "a surface is drawn in a three-dimensional axes"
    );
    assert_eq!(surface.tool(), "pan");
}

// Why: a page offers "Save" as a download, and the user expects back the kind of file they opened, holding the
// figure as it is now: a `.fig` stays protobuf and a `.fig.json` stays JSON, and either decodes to the figure that
// was opened when nothing has been changed. A save that silently changed encoding would break every tool the user
// feeds the file to next.
#[wasm_bindgen_test]
fn saving_returns_the_bytes_of_the_source_encoding() {
    for (bytes, name, codec) in [
        (LINE_MARKERS_FIG, "fig", Format::Fig),
        (LINE_MARKERS_JSON, "json", Format::Json),
    ] {
        let handle = headless(bytes, name);
        assert_eq!(
            handle.format(),
            name,
            "the handle remembers the encoding it was opened from"
        );
        let saved = handle.save();
        assert!(!saved.is_empty(), "a save has bytes");
        assert_eq!(
            decode(codec, &saved),
            decode(codec, bytes),
            "the {name} save decodes, with the {name} codec, to the figure that was opened"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Input without a GPU
// ---------------------------------------------------------------------------------------------------------------------

// Why: a page has only raw pointer events, and the handle must decide a press, a drag and a release by egui's own
// rules so that the figure behaves in the browser as it does in the viewer: a press decides nothing, a move within
// the click distance decides nothing, a move beyond it starts a pan at the press origin, and the release ends it
// with the change on the undo stack. None of this needs a surface, only an area to fit the page into, so the
// gesture logic is proved here without a GPU and the rendering test below only has to show that a real canvas
// reaches the same code. The saved bytes prove that what is saved is the figure as displayed, with the pan applied.
#[wasm_bindgen_test]
fn a_pan_on_a_headless_handle_follows_egui_s_rules_and_is_undoable() {
    let handle = headless(LINE_MARKERS_FIG, "fig");
    handle.resize(400, 300, 1.0);
    let source = decode(Format::Fig, LINE_MARKERS_FIG);

    let pressed = outcome(handle.pointer_down(200.0, 150.0));
    assert!(!pressed.redraw, "a press decides nothing: {pressed:?}");
    assert_eq!(
        pressed.cursor, "grab",
        "the hand stays open until a drag begins"
    );
    assert!(!pressed.can_undo);

    let nudged = outcome(handle.pointer_move(204.0, 150.0, 1));
    assert!(
        !nudged.redraw && !nudged.can_undo,
        "a move within egui's click distance (6 points) is not yet a drag: {nudged:?}"
    );

    let dragged = outcome(handle.pointer_move(260.0, 150.0, 1));
    assert!(
        dragged.redraw,
        "a move beyond the click distance pans the axes: {dragged:?}"
    );
    assert_eq!(dragged.cursor, "grabbing", "the hand closes while dragging");
    assert!(
        !dragged.can_undo,
        "the drag is one undo step, held open until it ends, so nothing is undoable yet: {dragged:?}"
    );
    assert!(!dragged.can_redo);
    assert!(dragged.rubber_band.is_none(), "Pan draws no rubber band");
    assert!(
        dragged.datatip.is_none(),
        "nothing is read under the pointer while dragging"
    );

    let released = outcome(handle.pointer_up(270.0, 150.0));
    assert!(
        released.redraw,
        "the release moves the pan to where the button came up: {released:?}"
    );
    assert_eq!(released.cursor, "grab", "the drag is over");
    assert!(
        released.can_undo,
        "the whole drag is one step on the undo stack"
    );

    let undone = outcome(handle.undo());
    assert!(
        undone.redraw,
        "undoing the pan changes the figure: {undone:?}"
    );
    assert!(
        !undone.can_undo && undone.can_redo,
        "one undo takes back the whole gesture: {undone:?}"
    );

    let redone = outcome(handle.redo());
    assert!(
        redone.redraw && redone.can_undo && !redone.can_redo,
        "{redone:?}"
    );

    let refitted = outcome(handle.refit());
    assert!(
        refitted.redraw,
        "Refit restores the view of every axes: {refitted:?}"
    );
    assert!(
        refitted.can_undo,
        "and is a step of its own on the undo stack"
    );

    let undid_refit = outcome(handle.undo());
    let undid_pan = outcome(handle.undo());
    let exhausted = outcome(handle.undo());
    assert!(
        undid_refit.redraw && undid_refit.can_undo,
        "{undid_refit:?}"
    );
    assert!(undid_pan.redraw && !undid_pan.can_undo, "{undid_pan:?}");
    assert!(
        !exhausted.redraw && !exhausted.can_undo && exhausted.can_redo,
        "an undo with nothing left to undo changes nothing: {exhausted:?}"
    );

    // Saving is left until last: the viewer's own save folds the overlay into the source and empties the undo
    // history, and whether the browser's save does the same is the host's decision, which nothing above depends on.
    assert_eq!(
        decode(Format::Fig, &handle.save()),
        source,
        "with everything undone, the save is the figure that was opened"
    );
    let _ = outcome(handle.redo());
    let panned = decode(Format::Fig, &handle.save());
    assert_ne!(
        panned.axes[0].x.limits, source.axes[0].x.limits,
        "what is saved is the figure as displayed, with the pan applied"
    );
    assert_eq!(
        panned.axes[0].y.limits, source.axes[0].y.limits,
        "a horizontal drag leaves the vertical limits alone"
    );
}

// Why: a second finger, a `pointercancel` from the browser or a lost capture ends a press without a release, and the
// handle must forget the press without reading it as a click: a click where the finger landed would toggle a legend
// entry the reader never meant to touch, and a change would appear on the undo stack. The press here lands on the
// legend entry of the first line, where a click would hide the line and be undoable; after the cancel there is
// nothing to undo, the figure is unchanged, and the next input starts afresh with the open hand.
#[wasm_bindgen_test]
fn a_cancelled_press_is_not_a_click() {
    let handle = headless(LINE_MARKERS_FIG, "fig");
    handle.resize(400, 300, 1.0);
    let before = handle.save();

    let legend = legend_entry(&handle);
    // The premise: a press and release here is a click that toggles the entry and is undoable.
    let _ = outcome(handle.pointer_down(legend.x, legend.y));
    let clicked = outcome(handle.pointer_up(legend.x, legend.y));
    assert!(
        clicked.redraw && clicked.can_undo,
        "a press and release on a legend entry toggles it: {clicked:?}"
    );
    let _ = outcome(handle.undo());
    assert_eq!(handle.save(), before, "and the undo restores the figure");

    // The test: the same press, cancelled instead of released.
    let pressed = outcome(handle.pointer_down(legend.x, legend.y));
    assert!(!pressed.redraw && !pressed.can_undo, "{pressed:?}");
    let cancelled = outcome(handle.pointer_cancel());
    assert!(
        !cancelled.redraw && !cancelled.can_undo,
        "a cancelled press toggles nothing and leaves nothing to undo: {cancelled:?}"
    );
    assert_eq!(cancelled.cursor, "grab", "no drag is in progress");
    assert_eq!(handle.save(), before, "the figure is exactly as it was");

    // A drag in progress is closed by a cancel where its last move left it, without a click.
    let _ = outcome(handle.pointer_down(200.0, 150.0));
    let dragged = outcome(handle.pointer_move(260.0, 150.0, 1));
    assert!(dragged.redraw, "{dragged:?}");
    let ended = outcome(handle.pointer_cancel());
    assert!(
        ended.can_undo,
        "the pan already made is kept as one step: {ended:?}"
    );
    assert_eq!(ended.cursor, "grab", "the drag is over");
    let after = outcome(handle.pointer_move(260.0, 150.0, 0));
    assert!(
        !after.redraw,
        "a move after the cancel is a hover, not a drag: {after:?}"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// The shape of an outcome
// ---------------------------------------------------------------------------------------------------------------------

// Why: a page draws its cursor, rubber band, datatip and toolbar state from the outcome of every input, so the
// outcome must be a plain object with exactly the documented keys, present even when null, or the page's
// destructuring would silently read `undefined`. Changing the tool is the one input that changes the cursor without
// changing the figure, and a tool name the host does not know must leave the tool as it was rather than throw
// into an event handler.
#[wasm_bindgen_test]
fn an_input_outcome_is_a_plain_object_with_the_documented_fields() {
    let handle = headless(LINE_MARKERS_FIG, "fig");

    let value = handle.pointer_leave();
    assert!(value.is_object(), "an outcome is an object: {value:?}");
    for key in OUTCOME_KEYS {
        assert!(
            js_sys::Reflect::has(&value, &JsValue::from_str(key)).expect("an object answers `has`"),
            "the outcome has the key {key:?} even when its value is null: {value:?}"
        );
    }
    let keys = js_sys::Object::keys(value.unchecked_ref::<js_sys::Object>());
    assert_eq!(
        keys.length() as usize,
        OUTCOME_KEYS.len(),
        "and no other keys: {keys:?}"
    );
    let at_rest = outcome(value);
    assert!(
        !at_rest.redraw,
        "leaving a canvas nothing was dragged on changes nothing"
    );
    assert_eq!(at_rest.cursor, "grab");
    assert!(at_rest.rubber_band.is_none());
    assert!(at_rest.datatip.is_none());
    assert!(!at_rest.can_undo && !at_rest.can_redo);
    assert_eq!(at_rest.problem_count, 0);

    let zoom = outcome(handle.set_tool("zoom"));
    assert_eq!(handle.tool(), "zoom");
    assert_eq!(
        zoom.cursor, "crosshair",
        "the cursor is the only sign of the tool before a drag"
    );
    assert!(
        !zoom.redraw,
        "changing the tool changes nothing in the figure"
    );

    let rotate = outcome(handle.set_tool("rotate"));
    assert_eq!(handle.tool(), "rotate");
    assert_eq!(rotate.cursor, "move");

    let nonsense = outcome(handle.set_tool("nonsense"));
    assert_eq!(
        handle.tool(),
        "rotate",
        "an unknown tool name is ignored and the tool is unchanged"
    );
    assert_eq!(
        nonsense.cursor, "move",
        "so the cursor reported is still the current tool's"
    );
    assert!(!nonsense.redraw);

    let pan = outcome(handle.set_tool("pan"));
    assert_eq!(handle.tool(), "pan");
    assert_eq!(pan.cursor, "grab");
}

// ---------------------------------------------------------------------------------------------------------------------
// Typesetting and export, without a GPU
// ---------------------------------------------------------------------------------------------------------------------

// Why: latex-rust parses and lays out recursively, and the browser's main thread has no helper thread to typeset
// on, so the deepest nesting the text engine admits is typeset on the wasm stack itself; the crate links with an
// 8 MiB stack for exactly this. latex-rust's nesting limit rejects deeper math with an error that the engine turns
// into a warning, so the deepest admitted depth is found by asking the engine, not assumed, and it must be at least
// the fifteen levels ironlab-text proves on a 256 KiB thread natively. That depth must typeset with no warning at
// all: an overflow would trap the test, and a fallback to plain text would surface as a warning here and as a
// problem through the handle. A real gallery figure whose every text is mathematics proves the same through the
// public API.
#[wasm_bindgen_test]
fn the_deepest_admitted_math_nesting_typesets_on_the_wasm_stack() {
    let text = TextEngine::new();
    let rejected = |depth: usize| {
        text.layout(&nested_fractions(depth), true, 9.0)
            .warnings
            .iter()
            .any(|w| w.message.contains("nests deeper than"))
    };
    let deepest = (1..=64)
        .take_while(|&depth| !rejected(depth))
        .last()
        .expect("one fraction is admitted");
    assert!(
        deepest >= 15,
        "the limit admits at least fifteen nested fractions, as ironlab-text's tests prove natively; it admits {deepest}"
    );
    assert!(
        rejected(deepest + 1),
        "and one level more is rejected by the nesting limit, not by the stack"
    );

    let source = nested_fractions(deepest);
    let layout = text.layout(&source, true, 9.0);
    assert!(
        layout.warnings.is_empty(),
        "{deepest} nested fractions typeset in the browser with no fallback: {:?}",
        layout.warnings
    );

    let figure = titled_figure(&source);
    let scene = ironlab_scene::compile(&figure, &text);
    assert!(
        scene.warnings.is_empty(),
        "a title of that depth compiles with no warning: {:?}",
        scene.warnings
    );

    let handle = headless(&figure.to_protobuf(), "fig");
    assert!(
        problems(&handle).is_empty(),
        "and through the handle the figure has no problems: {:?}",
        problems(&handle)
    );

    let labels = headless(LATEX_LABELS_FIG, "fig");
    assert!(
        problems(&labels).is_empty(),
        "a gallery figure of typeset titles, labels and legend has no problems in the browser: {:?}",
        problems(&labels)
    );
}

// Why: a figure with nothing to rasterise and no three-dimensional axes is exported without ever touching the GPU,
// so a page must be able to export a PDF when it has no surface at all, and the bytes it hands the download must be
// a PDF. Exporting reads the figure and must not add to or take from its problems. The same handle has no surface,
// so asking it to render is an error a page can report, not a silent no-op.
#[wasm_bindgen_test]
async fn exporting_a_vector_only_figure_needs_no_gpu_and_yields_a_pdf() {
    let handle = headless(LINE_MARKERS_FIG, "fig");
    let before = problems(&handle);
    assert!(before.is_empty(), "{before:?}");

    let value = JsFuture::from(handle.export_pdf())
        .await
        .expect("a vector-only figure exports without a GPU");
    let Export { bytes, warnings } = export(value);
    assert!(
        warnings.is_empty(),
        "nothing in a vector-only figure is rasterised or left unverified: {warnings:?}"
    );
    assert!(
        bytes.starts_with(b"%PDF-"),
        "the bytes are a PDF: they begin {:?}",
        String::from_utf8_lossy(&bytes[..bytes.len().min(8)])
    );
    let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(32)..]).into_owned();
    assert!(
        tail.contains("%%EOF"),
        "and end with the PDF trailer: {tail:?}"
    );

    assert_eq!(
        problems(&handle).len(),
        before.len(),
        "exporting neither adds nor removes problems"
    );
    assert!(
        handle.render().is_err(),
        "a handle opened without a canvas has no surface to render to"
    );
}

// Why: headless and no-GPU are first class: a three-dimensional figure needs an adapter only to verify its painter's
// order, and without one it is still exported, back to front with a warning, rather than refused. A page without
// WebGPU or WebGL2 must therefore still hand the user a PDF of a surface, and the warning that says the axes went
// unverified must reach the page with the bytes, naming the axes it concerns, so that the reader is told.
#[wasm_bindgen_test]
async fn a_three_dimensional_figure_exports_without_a_gpu_rather_than_failing() {
    let handle = headless(SURF_FIG, "fig");
    let value = JsFuture::from(handle.export_pdf())
        .await
        .expect("a surface exports without a GPU, unverified");
    let Export { bytes, warnings } = export(value);
    assert!(bytes.starts_with(b"%PDF-"), "the bytes are a PDF");
    assert_eq!(
        warnings.len(),
        1,
        "the one three-dimensional axes went unverified, and the page is told so: {warnings:?}"
    );
    let warning = &warnings[0];
    assert!(
        warning.detail.contains("adapter"),
        "the warning names the missing adapter: {warning:?}"
    );
    assert!(
        warning.subject.contains("Axes") || warning.subject.contains("axes"),
        "the warning names the axes it concerns: {warning:?}"
    );
    assert!(
        !warning.explanation.is_empty(),
        "and says how it arose: {warning:?}"
    );
}

// Why: a page shows the engine's version in its footer and in bug reports, and it must be the version of the
// crate that was compiled into the module, not a string that drifts from it.
#[wasm_bindgen_test]
fn version_is_the_crate_version() {
    assert_eq!(Session::version(), env!("CARGO_PKG_VERSION"));
}

// Why: a headless session exists so that a page on a machine without a GPU can still open, edit, save and export,
// but it has nothing to draw with, so it must say so when asked for a canvas rather than hand back a handle whose
// every render fails.
#[wasm_bindgen_test]
async fn a_headless_session_has_no_backend_and_cannot_open_a_figure_on_a_canvas() {
    let session = Session::headless();
    assert_eq!(
        session.backend(),
        "none",
        "a headless session names no graphics backend"
    );
    let canvas = canvas_element(400, 300);
    let opened = session
        .open(canvas.clone(), LINE_MARKERS_FIG.to_vec(), "fig".to_owned())
        .await;
    assert!(
        opened.is_err(),
        "there is no device to create a surface with"
    );
    canvas.remove();
}

// ---------------------------------------------------------------------------------------------------------------------
// Rendering on a canvas (a smoke test; skipped, not failed, without a graphics backend)
// ---------------------------------------------------------------------------------------------------------------------

// Why: everything above is proved without a surface; this shows that a real `<canvas>` reaches the same code: a
// session finds a backend, a figure opens on the canvas, the first frame renders, a pan through the pointer methods
// pans the figure the page then redraws, and the pan is undone. Headless Chrome without WebGPU or WebGL2 skips it
// with a warning, as the offscreen tests skip without an adapter, so a machine without a GPU still runs the suite.
#[wasm_bindgen_test]
async fn a_figure_opened_on_a_canvas_renders_and_a_pan_is_undoable() {
    let session = match Session::create().await {
        Ok(session) => session,
        Err(error) => {
            web_sys::console::warn_1(&JsValue::from_str(&format!(
                "skipping: neither WebGPU nor WebGL2 is available in this browser ({:?})",
                JsValue::from(error)
            )));
            return;
        }
    };
    let backend = session.backend();
    assert!(
        matches!(backend.as_str(), "webgpu" | "webgl2"),
        "a created session names the backend it found: {backend:?}"
    );

    let canvas = canvas_element(400, 300);
    let handle = session
        .open(canvas.clone(), LINE_MARKERS_FIG.to_vec(), "fig".to_owned())
        .await
        .map_err(JsValue::from)
        .expect("a gallery figure opens on the canvas");
    handle.resize(400, 300, 1.0);
    handle
        .render()
        .map_err(JsValue::from)
        .expect("the first frame renders");
    handle.set_backdrop(30, 30, 30);
    handle
        .render()
        .map_err(JsValue::from)
        .expect("a frame renders on a new backdrop");

    let _ = outcome(handle.pointer_down(200.0, 150.0));
    let _ = outcome(handle.pointer_move(204.0, 150.0, 1));
    let dragged = outcome(handle.pointer_move(260.0, 150.0, 1));
    assert!(dragged.redraw, "the pan changes the figure: {dragged:?}");
    let released = outcome(handle.pointer_up(270.0, 150.0));
    assert!(released.redraw && released.can_undo, "{released:?}");
    assert_eq!(released.cursor, "grab");
    handle
        .render()
        .map_err(JsValue::from)
        .expect("the panned figure renders");

    let undone = outcome(handle.undo());
    assert!(
        undone.redraw && !undone.can_undo && undone.can_redo,
        "{undone:?}"
    );
    handle
        .render()
        .map_err(JsValue::from)
        .expect("the restored figure renders");

    handle.release();
    canvas.remove();
}
