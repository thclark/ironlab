//! One open figure: the handle a page drives.
//!
//! A [`FigureHandle`] owns a [`FigureCanvas`] and the [`Pointer`] recogniser that turns the page's raw pointer events
//! into egui's gestures, and, when it was opened on a canvas, the device and surface it draws on. Every input method
//! answers with an [`Outcome`] as a plain JavaScript object; `render()` draws a frame; `save()` and `export_pdf()`
//! hand back the bytes of a download.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use emath::{Pos2, Rect, pos2, vec2};
use ironlab_canvas::files::Format;
use ironlab_canvas::offscreen::{OffscreenRenderer, RenderError};
use ironlab_canvas::problems::{Origin, Problem};
use ironlab_canvas::{
    DrawList, FigureCanvas, Gesture, MAX_TILE_SIDE, Pointer, Viewport, export_display_list,
    wheel_factor,
};
use ironlab_ir::Figure;
use ironlab_pdf::PdfOptions;
use ironlab_text::TextEngine;
use js_sys::{Object, Promise, Reflect, Uint8Array};
use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise};

use crate::error::WebError;
use crate::gpu::{Gpu, Screen};
use crate::outcome::{Outcome, tool_name, tool_named};

/// What the handle holds behind its reference count.
struct Inner {
    canvas: FigureCanvas,
    pointer: Pointer,
    text: Rc<TextEngine>,
    format: Format,
    /// The device and surface of the canvas, or `None` for a handle opened headless or released.
    gpu: Option<(Gpu, Screen)>,
    /// Device pixels per CSS pixel, as the last `resize` gave it.
    dpr: f32,
    /// The colour the canvas is cleared to where the figure's background is not opaque.
    backdrop: [u8; 3],
    /// The address of the draw list last drawn, so that a new list lets the painter drop what is no longer used.
    last_list: Option<usize>,
    released: bool,
}

/// One entry of `problems()`, and of the warnings of an export, as the page reads it.
#[derive(Serialize)]
struct Entry {
    subject: String,
    detail: String,
    explanation: String,
}

/// How an export warning arose, shown under it as the problems list shows its explanations.
const EXPORT_EXPLANATION: &str = "Reported while the figure was exported.";

/// The time now, in seconds, for the pointer recogniser's thresholds.
fn now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or_else(js_sys::Date::now, |performance| performance.now())
        / 1000.0
}

/// Serialises `value` as a plain JavaScript object, with absent options as `null`.
fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

/// One open figure, as a page drives it.
///
/// Input positions are CSS pixels from the canvas's top-left corner; the outcome's geometry is in the same space.
#[wasm_bindgen]
pub struct FigureHandle(Rc<RefCell<Inner>>);

impl FigureHandle {
    /// A handle on `figure`, decoded from `format`, drawing through `gpu` when it has one.
    #[must_use]
    pub(crate) fn new(
        figure: Figure,
        format: Format,
        text: Rc<TextEngine>,
        gpu: Option<(Gpu, Screen)>,
    ) -> Self {
        // A figure in a document fills the box the document gives it, which is sized to the page's aspect ratio,
        // so the page is fitted with no margin and covers the canvas exactly.
        let mut canvas = FigureCanvas::new(figure);
        canvas.set_margin(0.0);
        Self(Rc::new(RefCell::new(Inner {
            canvas,
            pointer: Pointer::default(),
            text,
            format,
            gpu,
            dpr: 1.0,
            backdrop: [43, 43, 46],
            last_list: None,
            released: false,
        })))
    }
}

impl Inner {
    /// Forwards `gestures` to the canvas and returns whether any changed the figure.
    fn forward(&mut self, gestures: Vec<Gesture>) -> bool {
        let text = Rc::clone(&self.text);
        let mut changed = false;
        for gesture in gestures {
            changed |= match gesture {
                Gesture::DragStart(pos) => {
                    self.canvas.drag_start(pos, &text);
                    false
                }
                Gesture::DragMove(pos) => self.canvas.drag_move(pos),
                Gesture::DragEnd(pos) => self.canvas.drag_end(pos),
                Gesture::Click(pos) => self.canvas.click(pos, &text),
                Gesture::DoubleClick(pos) => self.canvas.double_click(pos, &text),
            };
        }
        changed
    }

    /// The outcome of the canvas as it now stands.
    fn outcome(&mut self, redraw: bool) -> JsValue {
        let text = Rc::clone(&self.text);
        to_js(&Outcome::from_canvas(&mut self.canvas, &text, redraw))
    }

    /// Everything wrong with the figure, as the page lists it.
    fn problems(&mut self) -> Vec<Entry> {
        let text = Rc::clone(&self.text);
        let problems = self.canvas.problems(&text);
        let figure = self.canvas.state().figure();
        problems
            .iter()
            .map(|problem| Entry {
                subject: problem.subject(figure),
                detail: problem.detail.clone(),
                explanation: problem.origin.explanation().to_owned(),
            })
            .collect()
    }

    /// Draws one frame.
    fn render(&mut self) -> Result<(), WebError> {
        let text = Rc::clone(&self.text);
        let (gpu, screen) = self.gpu.as_mut().ok_or(WebError::NoSurface)?;
        let drawn = self.canvas.draw_list(&text);
        let clear = match self.canvas.background(&text) {
            Some(colour) if colour[3] == 255 => colour,
            _ => [self.backdrop[0], self.backdrop[1], self.backdrop[2], 255],
        };
        let to_screen = drawn.as_ref().map_or(
            ironlab_canvas::ScreenTransform {
                scale: 1.0,
                origin: Pos2::ZERO,
            },
            |(_, to_screen)| *to_screen,
        );
        let viewport = Viewport::whole(screen.size(), self.dpr, to_screen);
        let list: Option<&Arc<DrawList>> = drawn.as_ref().map(|(list, _)| list);
        // A new list means the last one is no longer drawn: what the painter has not prepared since its previous
        // round is dropped, and the round begins again.
        let address = list.map(|list| Arc::as_ptr(list).addr());
        if address != self.last_list {
            gpu.painter.borrow_mut().retain_used();
            self.last_list = address;
        }
        screen.frame(gpu, list, &viewport, clear)
    }
}

#[wasm_bindgen]
impl FigureHandle {
    /// Gives the canvas `width_px` by `height_px` device pixels at `dpr` device pixels per CSS pixel: the surface is
    /// configured for that size, and the figure is fitted into the canvas's area in CSS pixels. A page calls this
    /// before its first `render`, and again whenever the canvas's size or the device pixel ratio changes.
    pub fn resize(&self, width_px: u32, height_px: u32, dpr: f32) {
        let mut inner = self.0.borrow_mut();
        let dpr = if dpr.is_finite() && dpr > 0.0 {
            dpr
        } else {
            1.0
        };
        inner.dpr = dpr;
        let max_tile_side = match &mut inner.gpu {
            Some((gpu, screen)) => {
                screen.resize(&gpu.device, width_px, height_px);
                gpu.max_texture_side()
            }
            None => MAX_TILE_SIDE,
        };
        let area = Rect::from_min_size(
            Pos2::ZERO,
            vec2(width_px as f32 / dpr, height_px as f32 / dpr),
        );
        inner.canvas.resize(area, max_tile_side);
    }

    /// Sets the colour the canvas is cleared to where the figure's background is not opaque.
    pub fn set_backdrop(&self, r: u8, g: u8, b: u8) {
        self.0.borrow_mut().backdrop = [r, g, b];
    }

    /// Draws one frame on the canvas.
    ///
    /// # Errors
    ///
    /// Throws when the handle has no canvas (opened headless, or released), when `resize` has not yet given the
    /// canvas a size, when the surface cannot provide a frame, or when the device reported an error.
    pub fn render(&self) -> Result<(), JsError> {
        let mut inner = self.0.borrow_mut();
        if inner.released {
            return Err(WebError::NoSurface.into());
        }
        inner.render().map_err(JsError::from)
    }

    /// The primary button was pressed at (`x`, `y`).
    pub fn pointer_down(&self, x: f32, y: f32) -> JsValue {
        let mut inner = self.0.borrow_mut();
        let pos = pos2(x, y);
        inner.canvas.hover(Some(pos));
        let gestures = inner.pointer.down(pos, now());
        let changed = inner.forward(gestures);
        inner.outcome(changed)
    }

    /// The pointer moved to (`x`, `y`) with `buttons` held, as the DOM reports them: bit 0 is the primary button. A
    /// move with the primary button up ends any press whose release was lost, and is otherwise a hover.
    pub fn pointer_move(&self, x: f32, y: f32, buttons: u16) -> JsValue {
        let mut inner = self.0.borrow_mut();
        let pos = pos2(x, y);
        inner.canvas.hover(Some(pos));
        let time = now();
        let gestures = if buttons & 1 == 0 {
            inner.pointer.up(None, time)
        } else {
            inner.pointer.moved(pos, time)
        };
        let changed = inner.forward(gestures);
        inner.outcome(changed)
    }

    /// The primary button was released at (`x`, `y`), which ends a drag or registers a click.
    pub fn pointer_up(&self, x: f32, y: f32) -> JsValue {
        let mut inner = self.0.borrow_mut();
        let pos = pos2(x, y);
        inner.canvas.hover(Some(pos));
        let gestures = inner.pointer.up(Some(pos), now());
        let changed = inner.forward(gestures);
        inner.outcome(changed)
    }

    /// The press in progress was cancelled: a drag ends where its last move left it, a press that had not become a
    /// drag is forgotten, and neither is a click.
    pub fn pointer_cancel(&self) -> JsValue {
        let mut inner = self.0.borrow_mut();
        let gestures = inner.pointer.up(None, now());
        let changed = inner.forward(gestures);
        inner.outcome(changed)
    }

    /// The pointer left the canvas: nothing is under it until it comes back.
    pub fn pointer_leave(&self) -> JsValue {
        let mut inner = self.0.borrow_mut();
        inner.canvas.hover(None);
        inner.outcome(false)
    }

    /// A wheel event at (`x`, `y`): `delta_y_css_px` is the DOM's `deltaY` in CSS pixels, positive when the reader
    /// scrolls down, and `pinch` the ratio a pinch scaled by (1 when there was none). Scrolling up zooms in, as it
    /// does in the viewer.
    pub fn wheel(&self, x: f32, y: f32, delta_y_css_px: f32, pinch: f32) -> JsValue {
        let mut inner = self.0.borrow_mut();
        let pos = pos2(x, y);
        inner.canvas.hover(Some(pos));
        let text = Rc::clone(&inner.text);
        let factor = wheel_factor(-delta_y_css_px, pinch);
        let changed = inner.canvas.wheel(pos, factor, &text);
        inner.outcome(changed)
    }

    /// Sets the tool a drag uses: `pan`, `zoom` or `rotate`. Any other name leaves the tool as it is.
    pub fn set_tool(&self, name: &str) -> JsValue {
        let mut inner = self.0.borrow_mut();
        if let Some(tool) = tool_named(name) {
            inner.canvas.set_tool(tool);
        }
        inner.outcome(false)
    }

    /// Undoes the last change.
    pub fn undo(&self) -> JsValue {
        let mut inner = self.0.borrow_mut();
        let changed = inner.canvas.undo();
        inner.outcome(changed)
    }

    /// Redoes the last undone change.
    pub fn redo(&self) -> JsValue {
        let mut inner = self.0.borrow_mut();
        let changed = inner.canvas.redo();
        inner.outcome(changed)
    }

    /// Restores the limits and three-dimensional views of every axes, as the toolbar's Refit does.
    pub fn refit(&self) -> JsValue {
        let mut inner = self.0.borrow_mut();
        let changed = inner.canvas.reset_view();
        inner.outcome(changed)
    }

    /// The tool a drag uses: `pan`, `zoom` or `rotate`.
    #[must_use]
    pub fn tool(&self) -> String {
        tool_name(self.0.borrow().canvas.state().tool).to_owned()
    }

    /// Whether there is a change to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.0.borrow().canvas.can_undo()
    }

    /// Whether there is an undone change to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.0.borrow().canvas.can_redo()
    }

    /// Whether the figure has a three-dimensional axes, which is what the Rotate tool turns.
    #[must_use]
    pub fn has_3d(&self) -> bool {
        self.0.borrow().canvas.has_3d()
    }

    /// Everything wrong with the figure as shown, as an array of `{ subject, detail, explanation }`.
    #[must_use]
    pub fn problems(&self) -> JsValue {
        to_js(&self.0.borrow_mut().problems())
    }

    /// The size of the page in points, as `[width_pt, height_pt]`.
    #[must_use]
    pub fn size_pt(&self) -> Vec<f32> {
        let mut inner = self.0.borrow_mut();
        let text = Rc::clone(&inner.text);
        let list = &inner.canvas.scene(&text).display_list;
        vec![list.width_pt as f32, list.height_pt as f32]
    }

    /// The figure as shown, encoded in the format it was opened from. The overlay of the reader's changes is not
    /// folded into the source: what was undoable before the save is undoable after it.
    #[must_use]
    pub fn save(&self) -> Vec<u8> {
        let inner = self.0.borrow();
        inner.format.encode(inner.canvas.state().figure())
    }

    /// The format the figure was opened from: `fig` or `json`.
    #[must_use]
    pub fn format(&self) -> String {
        self.0.borrow().format.extension().to_owned()
    }

    /// Exports the figure as shown to a PDF, resolving to `{ bytes: Uint8Array, warnings: [{ subject, detail,
    /// explanation }] }`, where the warnings are the exporter's: what it rasterised, and what it could not verify.
    ///
    /// The renders the exporter needs are drawn through the handle's device. A handle without one exports without a
    /// renderer: a figure that needs one only to verify its three-dimensional axes is exported back to front with a
    /// warning, and a figure that must be rasterised rejects.
    pub fn export_pdf(&self) -> Promise {
        let inner = Rc::clone(&self.0);
        future_to_promise(async move {
            // The cell is borrowed only between awaits, so that a pointer event arriving mid-export is served.
            let (list, options, text, gpu) = {
                let mut inner = inner.borrow_mut();
                let text = Rc::clone(&inner.text);
                let options = PdfOptions::for_figure(inner.canvas.state().figure());
                let list = inner.canvas.scene(&text).display_list.clone();
                let gpu = inner.gpu.as_ref().map(|(gpu, _)| gpu.clone());
                (list, options, text, gpu)
            };
            let rendered = match gpu {
                Some(gpu) => {
                    let mut renderer = OffscreenRenderer::from_device(
                        gpu.device.clone(),
                        gpu.queue.clone(),
                        gpu.sample_count,
                    );
                    let pump = Pump::start(gpu.device.clone());
                    let rendered =
                        export_display_list(&list, &text, &options, Ok(&mut renderer)).await;
                    pump.stop();
                    rendered
                }
                None => {
                    export_display_list(
                        &list,
                        &text,
                        &options,
                        Err(RenderError::NoAdapter(
                            "the session has no graphics device".to_owned(),
                        )),
                    )
                    .await
                }
            }
            .map_err(WebError::from)
            .map_err(JsError::from)?;
            let warnings: Vec<Entry> = {
                let inner = inner.borrow();
                let figure = inner.canvas.state().figure();
                rendered
                    .warnings
                    .iter()
                    .map(|warning| Entry {
                        subject: Problem {
                            origin: Origin::Scene,
                            node: warning.node,
                            path: None,
                            detail: String::new(),
                        }
                        .subject(figure),
                        detail: warning.message.clone(),
                        explanation: EXPORT_EXPLANATION.to_owned(),
                    })
                    .collect()
            };
            let result = Object::new();
            Reflect::set(
                &result,
                &"bytes".into(),
                &Uint8Array::from(rendered.bytes.as_slice()),
            )?;
            Reflect::set(&result, &"warnings".into(), &to_js(&warnings))?;
            Ok(result.into())
        })
    }

    /// Releases the canvas's surface and device. The handle stays readable, but it can no longer render.
    pub fn release(&self) {
        let mut inner = self.0.borrow_mut();
        inner.gpu = None;
        inner.released = true;
    }
}

/// Polls a device between turns of the event loop while an export runs, so that a readback the device completes
/// under WebGL, where the poll cannot wait, runs its callback and the awaited render finishes.
struct Pump {
    running: Rc<Cell<bool>>,
}

impl Pump {
    /// The interval between polls, in milliseconds.
    const INTERVAL_MS: i32 = 8;

    fn start(device: wgpu::Device) -> Self {
        let running = Rc::new(Cell::new(true));
        let flag = Rc::clone(&running);
        wasm_bindgen_futures::spawn_local(async move {
            while flag.get() {
                let _ = device.poll(wgpu::PollType::Poll);
                sleep(Self::INTERVAL_MS).await;
            }
        });
        Self { running }
    }

    fn stop(&self) {
        self.running.set(false);
    }
}

/// Waits `ms` milliseconds of the event loop.
async fn sleep(ms: i32) {
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        } else {
            let _ = resolve.call0(&JsValue::NULL);
        }
    });
    let _ = JsFuture::from(promise).await;
}
