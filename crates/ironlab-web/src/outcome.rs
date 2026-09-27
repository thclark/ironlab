//! What a page reads after every input: the [`Outcome`] that tells it what its chrome should now show, and the stem a
//! download is named by. Both are ordinary Rust, compiled on every target and proved by native tests.

use emath::Pos2;
use ironlab_canvas::figure_canvas::{Callout, Cursor, Marker as CanvasMarker};
use ironlab_canvas::files::figure_stem;
use ironlab_canvas::{FigureCanvas, Tool};
use ironlab_text::TextEngine;
use serde::Serialize;

/// The mark a page draws on its canvas at the data a datatip names, in CSS pixels from the canvas's top-left corner.
///
/// It serialises with a `kind` tag, so that the page reads `marker.kind` to choose between a circle and a polygon.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Marker {
    /// A ring of radius `r` about (`cx`, `cy`): around a drawn point, or around the centre of a pixel too small to
    /// outline.
    Ring { cx: f32, cy: f32, r: f32 },
    /// The four corners of a pixel, in the order they are joined.
    Outline { points: [[f32; 2]; 4] },
}

/// What a page shows for the data under the pointer.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Datatip {
    /// The lines of the callout, joined by newlines.
    pub text: String,
    /// Where the page anchors its tooltip, in CSS pixels: the centre of a ring, or the centroid of an outline.
    pub anchor: [f32; 2],
    /// The mark drawn at the data.
    pub marker: Marker,
}

/// The outcome of one input: everything the chrome around the canvas is drawn from.
///
/// Every field is present in every outcome, the optional ones as `null`, so that a page destructures it without
/// checks. `redraw` is whether the input changed the figure, exactly as the canvas reported it, so a page redraws
/// and announces a change on `redraw` alone and never infers either.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Outcome {
    /// Whether the figure changed and the canvas must be drawn again.
    pub redraw: bool,
    /// The CSS cursor keyword to show over the canvas.
    pub cursor: &'static str,
    /// The rubber band of a Zoom-tool drag as `[x, y, width, height]` in CSS pixels, normalised so that the size is
    /// never negative, or `None` when there is none.
    pub rubber_band: Option<[f32; 4]>,
    /// The datatip of the data under the pointer, or `None` when there is none.
    pub datatip: Option<Datatip>,
    /// Whether there is a change to undo.
    pub can_undo: bool,
    /// Whether there is an undone change to redo.
    pub can_redo: bool,
    /// How many problems the figure has, for the indicator; the list itself is fetched on demand.
    pub problem_count: usize,
}

impl Outcome {
    /// Reads the outcome of `canvas` as it now stands, with `redraw` as the gesture reported it.
    ///
    /// The canvas compiles its scene if it is out of date, since the rubber band, the callout and the problems are
    /// read against the current compilation.
    pub fn from_canvas(canvas: &mut FigureCanvas, text: &TextEngine, redraw: bool) -> Self {
        Self {
            redraw,
            cursor: cursor_keyword(canvas.cursor()),
            rubber_band: canvas.rubber_band(text).map(|band| {
                let band = normalised(band);
                [band.min.x, band.min.y, band.width(), band.height()]
            }),
            datatip: canvas.callout(text).map(datatip),
            can_undo: canvas.can_undo(),
            can_redo: canvas.can_redo(),
            problem_count: canvas.problems(text).len(),
        }
    }
}

/// A rectangle whose `min` is its top-left corner whichever way it was dragged out.
fn normalised(rect: emath::Rect) -> emath::Rect {
    emath::Rect::from_two_pos(rect.min, rect.max)
}

/// The CSS keyword of a cursor.
fn cursor_keyword(cursor: Cursor) -> &'static str {
    match cursor {
        Cursor::Grab => "grab",
        Cursor::Grabbing => "grabbing",
        Cursor::Crosshair => "crosshair",
        Cursor::Move => "move",
    }
}

/// The datatip of a callout: the mark and, from it, the anchor of the tooltip.
fn datatip(callout: Callout) -> Datatip {
    let (marker, anchor) = match callout.marker {
        CanvasMarker::Ring { centre, radius } => (
            Marker::Ring {
                cx: centre.x,
                cy: centre.y,
                r: radius,
            },
            [centre.x, centre.y],
        ),
        CanvasMarker::Outline(corners) => {
            let centroid = corners
                .iter()
                .fold(Pos2::ZERO, |sum, corner| sum + corner.to_vec2())
                / 4.0;
            (
                Marker::Outline {
                    points: corners.map(|corner| [corner.x, corner.y]),
                },
                [centroid.x, centroid.y],
            )
        }
    };
    Datatip {
        text: callout.text,
        anchor,
        marker,
    }
}

/// The name of a tool as the page spells it: `pan`, `zoom` or `rotate`.
#[must_use]
pub fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Pan => "pan",
        Tool::Zoom => "zoom",
        Tool::Rotate => "rotate",
    }
}

/// The tool named `name`, or `None` when the name is not one of `pan`, `zoom` and `rotate`.
#[must_use]
pub fn tool_named(name: &str) -> Option<Tool> {
    match name {
        "pan" => Some(Tool::Pan),
        "zoom" => Some(Tool::Zoom),
        "rotate" => Some(Tool::Rotate),
        _ => None,
    }
}

/// The stem a download of the figure opened from `name` is given: the last path component without its figure
/// extension (`.fig`, `.json` or `.fig.json`, in any case), or `figure` when nothing is left.
#[must_use]
pub fn download_stem(name: &str) -> &str {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = figure_stem(file);
    // `figure_stem` leaves a name that is only an extension, such as `.fig`, unchanged; prefixing a letter tells
    // that apart from a name with no extension at all.
    let only_extension = stem == file && figure_stem(&format!("a{file}")) == "a";
    if stem.is_empty() || only_extension {
        "figure"
    } else {
        stem
    }
}
