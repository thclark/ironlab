/**
 * The `<ironlab-figure>` element: an IronLAB figure shown in a page.
 *
 * The element fetches a figure file, hands it to the wasm module (`ironlab_core.js` and `ironlab_core_bg.wasm`),
 * which draws it on a canvas, and draws the toolbar ("figurebar") around the canvas as ordinary HTML buttons. Every
 * gesture is forwarded to the wasm in CSS pixels relative to the canvas, and the wasm answers each with an
 * `Outcome` that says what the chrome around the canvas should now show.
 *
 * Plain ES2022, no framework. The build script makes two textual substitutions, and the placeholders appear only
 * where they are substituted (never in a comment, because the substituted CSS contains comment terminators):
 *   - the SHADOW CSS placeholder, the value of the `SHADOW_CSS` constant below, receives the contents of
 *     `figure.css`, JSON-escaped, so that the string literal holds the shadow styles and one fetch styles the element;
 *   - the BUILD placeholder, in the import specifier and the wasm URL, receives a content hash, so that the wasm
 *     and its glue are fetched afresh after a build.
 *
 * Licensed under the AGPL-3.0-or-later, as the IronLAB crates are.
 */

import init, { Session } from "./ironlab_core.js?v=__IRONLAB_BUILD__";

const SHADOW_CSS = "__IRONLAB_SHADOW_CSS__";

/** How many CSS pixels a wheel event measured in lines (`deltaMode === 1`) counts per line. */
const LINE_HEIGHT_PX = 16;

/** The offset, in CSS pixels, between the datatip anchor and the near corner of the tooltip. */
const DATATIP_OFFSET = 14;

const STATUS_NO_GPU = "This figure needs WebGPU or WebGL2 to be interactive.";
const HINT_WHEEL = "Click to zoom with the wheel";

const MIME = {
  fig: "application/x-protobuf",
  json: "application/json",
  pdf: "application/pdf",
};

/** The cursor of each tool at rest, as `FigureCanvas::cursor` reports it, shown as soon as the tool is chosen. */
const CURSORS = { pan: "grab", zoom: "crosshair", rotate: "move" };

// =====================================================================================================================
// The session: one wasm instance and one GPU session per page
// =====================================================================================================================

let sessionPromise = null;

/**
 * The page's one `Session`, created on first use. `init` runs once and `Session.create()` runs once; a rejection is
 * remembered, so that after the first failure every element shows its fallback at once instead of trying again.
 * @returns {Promise<Session>}
 */
export function session() {
  if (sessionPromise === null) {
    sessionPromise = (async () => {
      await init({
        module_or_path: new URL("ironlab_core_bg.wasm?v=__IRONLAB_BUILD__", import.meta.url),
      });
      return Session.create();
    })();
  }
  return sessionPromise;
}

/** The version of the wasm module, once it has loaded. @returns {Promise<string>} */
export async function version() {
  await session();
  return Session.version();
}

// =====================================================================================================================
// The figurebar
// =====================================================================================================================

/**
 * The markup of one figurebar button. As the native toolbar does, every button carries its caption as words alone, so
 * it needs no `aria-label`.
 */
function button(action, caption, { parts = "", attrs = "" } = {}) {
  return (
    `<button type="button" part="button ${parts}" data-action="${action}" ${attrs}>` +
    `<span part="caption">${caption}</span></button>`
  );
}

// The toolbar in the native order: the tools and the history at the left, the controls that act on the whole figure
// at the right, wrapping to a second row when the row is too narrow for both groups.
const FIGUREBAR_HTML =
  `<div part="figurebar" role="toolbar" aria-label="Figure tools" hidden>` +
  `<div part="group tools">` +
  button("pan", "Pan", {
    parts: "tool",
    attrs: 'aria-pressed="true" title="Drag to pan 2D axes or move 3D axes. Scroll to zoom."',
  }) +
  button("zoom", "Zoom", {
    parts: "tool",
    attrs: 'aria-pressed="false" title="Drag a rectangle to zoom 2D axes to it. Scroll to zoom."',
  }) +
  button("rotate", "Rotate", {
    parts: "tool",
    attrs: 'aria-pressed="false" hidden title="Drag to rotate 3D axes. Scroll to zoom."',
  }) +
  button("refit", "Refit", {
    attrs:
      'title="Restore the limits and 3D views of every axes (R), keeping hidden plots hidden. ' +
      'Double-click an axes to restore only that axes."',
  }) +
  `<span part="separator" role="separator" aria-orientation="vertical"></span>` +
  button("undo", "Undo", { attrs: 'disabled title="Undo the last change (Cmd+Z, Ctrl+Z)."' }) +
  button("redo", "Redo", {
    attrs: 'disabled title="Redo the last undone change (Cmd+Shift+Z, Ctrl+Shift+Z)."',
  }) +
  `</div>` +
  `<span part="spacer"></span>` +
  `<div part="group file">` +
  button("problems", "0 problems", {
    parts: "quiet problems-button",
    attrs:
      'hidden aria-expanded="false" aria-controls="problems" popovertarget="problems" ' +
      'title="Show what is wrong with this figure."',
  }) +
  `<span part="separator" role="separator" aria-orientation="vertical" hidden data-for="problems"></span>` +
  button("save", "Save figure…", {
    attrs: 'title="Save the figure, as currently shown, to a .fig (Protocol Buffers) or .json file."',
  }) +
  button("export", "Export PDF…", {
    attrs: 'title="Save the figure, as currently shown, to a PDF file."',
  }) +
  `</div>` +
  `</div>` +
  `<div part="problems" id="problems" popover role="dialog" aria-label="Problems with this figure">` +
  `<ol part="problem-list"></ol>` +
  `</div>`;

const FRAME_HTML =
  `<div part="frame" data-state="loading" data-active="false">` +
  `<slot></slot>` +
  `<canvas part="canvas" tabindex="0" role="img"></canvas>` +
  `<div part="band" hidden></div>` +
  `<svg part="overlay" aria-hidden="true" focusable="false" hidden>` +
  `<circle part="marker ring" hidden></circle><polygon part="marker outline" hidden></polygon>` +
  `</svg>` +
  `<div part="datatip" role="tooltip" aria-live="polite" hidden></div>` +
  `<div part="hint" aria-hidden="true">${HINT_WHEEL}</div>` +
  `<div part="status" role="status" hidden></div>` +
  `<span part="probe"></span>` +
  `</div>`;

// =====================================================================================================================
// Styles: one constructed stylesheet shared by every element, or a <style> where that is not supported
// =====================================================================================================================

let sharedSheet = null;

function adoptStyles(root) {
  if ("adoptedStyleSheets" in root && typeof CSSStyleSheet === "function") {
    try {
      if (sharedSheet === null) {
        sharedSheet = new CSSStyleSheet();
        sharedSheet.replaceSync(SHADOW_CSS);
      }
      root.adoptedStyleSheets = [sharedSheet];
      return;
    } catch {
      // Older engines construct the sheet but refuse to adopt it: fall through to a <style>.
    }
  }
  const style = document.createElement("style");
  style.textContent = SHADOW_CSS;
  root.append(style);
}

// =====================================================================================================================
// Helpers
// =====================================================================================================================

function messageOf(error) {
  if (error instanceof Error) return error.message;
  return String(error);
}

function formatOf(src) {
  const path = src.split(/[?#]/, 1)[0];
  if (/\.json$/i.test(path)) return "json";
  return "fig";
}

function nameOf(src) {
  const path = src.split(/[?#]/, 1)[0];
  let file = path.slice(path.lastIndexOf("/") + 1);
  try {
    file = decodeURIComponent(file);
  } catch {
    // A file name that is not valid percent-encoding is kept as written.
  }
  return file.replace(/\.fig\.json$|\.fig$|\.json$/i, "") || "figure";
}

function problemsCaption(count) {
  return count === 1 ? "1 problem" : `${count} problems`;
}

/** Parses the colour a browser reports for a computed `color`: `rgb()`, `rgba()` or `color(srgb …)`. */
function parseColour(computed) {
  let m = /^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)/.exec(computed);
  if (m) return [Math.round(+m[1]), Math.round(+m[2]), Math.round(+m[3])];
  m = /^color\(srgb\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)/.exec(computed);
  if (m) return [Math.round(255 * +m[1]), Math.round(255 * +m[2]), Math.round(255 * +m[3])];
  return null;
}

function download(root, bytes, filename, mime) {
  const blob = new Blob([bytes], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.hidden = true;
  root.append(a);
  a.click();
  a.remove();
  // The browser reads the blob when the click is handled; a revoke on the next turn is early enough for Safari
  // only if it is deferred a little.
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

// =====================================================================================================================
// The element
// =====================================================================================================================

export class IronlabFigure extends HTMLElement {
  static observedAttributes = ["theme", "alt", "toolbar"];

  /** Defines the element once; calling it again, or loading the module twice, is harmless. */
  static define(tag = "ironlab-figure") {
    if (!customElements.get(tag)) customElements.define(tag, IronlabFigure);
  }

  #root;
  #els = null;
  #handle = null;
  #loading = false;
  #rendered = false;
  #sized = false;
  #dirty = false;
  #raf = 0;
  #intersection = null;
  #resize = null;
  #mutation = null;
  #media = null;
  #resolution = null;
  #pointers = new Map();
  #pinchDistance = 0;
  #wheelActive = false;
  #backdrop = null;
  #ready;
  #resolveReady;
  #rejectReady;
  #settled = false;

  constructor() {
    super();
    this.#root = this.attachShadow({ mode: "open" });
    this.#ready = new Promise((resolve, reject) => {
      this.#resolveReady = resolve;
      this.#rejectReady = reject;
    });
    // A page that never awaits `ready` must not see an unhandled rejection when the figure cannot load.
    this.#ready.catch(() => {});
  }

  // ----- attributes --------------------------------------------------------------------------------------------------

  get src() {
    return this.getAttribute("src") ?? "";
  }

  /** The figure's name: the `name` attribute, or the file name of `src` without `.fig`, `.json` or `.fig.json`. */
  get name() {
    return this.getAttribute("name") || nameOf(this.src);
  }

  get alt() {
    return this.getAttribute("alt") || this.name;
  }

  /** The file's encoding, `fig` or `json`: the `format` attribute, or the last extension of `src`. */
  get format() {
    const given = (this.getAttribute("format") || "").toLowerCase();
    return given === "fig" || given === "json" ? given : formatOf(this.src);
  }

  get theme() {
    const given = (this.getAttribute("theme") || "auto").toLowerCase();
    return given === "light" || given === "dark" ? given : "auto";
  }

  get toolbar() {
    return (this.getAttribute("toolbar") || "auto").toLowerCase() === "hidden" ? "hidden" : "auto";
  }

  // ----- public API --------------------------------------------------------------------------------------------------

  /** Resolves with the `FigureHandle` once the figure has been drawn for the first time; rejects if it cannot be. */
  get ready() {
    return this.#ready;
  }

  /** The `FigureHandle`, or `null` before the figure is open and after the element is disconnected. */
  get handle() {
    return this.#handle;
  }

  /** The page's `Session` promise, shared by every element. */
  get session() {
    return session();
  }

  /** Restores the limits and three-dimensional views of every axes, as the Refit button does. */
  refit() {
    if (!this.#handle) return;
    this.#apply(this.#handle.refit());
  }

  /** Downloads the figure as currently shown, in the encoding it was loaded from. */
  save() {
    const handle = this.#requireHandle();
    const format = handle.format();
    download(this.#root, handle.save(), `${this.name}.${format}`, MIME[format] ?? "application/octet-stream");
  }

  /**
   * Exports the figure as currently shown to a PDF and downloads it. The exporter's warnings (what it rasterised, and
   * what it could not verify) are shown in the status line, logged, and dispatched as `ironlab-export`. Rejects, after
   * reporting, if the export fails.
   */
  async exportPdf() {
    const handle = this.#requireHandle();
    const control = this.#els.export;
    control.disabled = true;
    control.setAttribute("aria-busy", "true");
    try {
      const { bytes, warnings } = await handle.export_pdf();
      download(this.#root, bytes, `${this.name}.pdf`, MIME.pdf);
      if (warnings.length > 0) {
        const plural = warnings.length === 1 ? "warning" : "warnings";
        this.#status(`Exported with ${warnings.length} ${plural}: ${warnings[0].subject}`);
        console.warn(`ironlab-figure: ${this.name}.pdf exported with ${plural}`, warnings);
      } else {
        this.#status("");
      }
      this.dispatchEvent(new CustomEvent("ironlab-export", { bubbles: true, composed: true, detail: { warnings } }));
      return warnings;
    } catch (error) {
      this.#status(`The PDF could not be exported: ${messageOf(error)}`);
      console.error("ironlab-figure: export failed", error);
      throw error;
    } finally {
      control.disabled = false;
      control.removeAttribute("aria-busy");
    }
  }

  #requireHandle() {
    if (!this.#handle) throw new Error("The figure is not loaded.");
    return this.#handle;
  }

  // ----- lifecycle ---------------------------------------------------------------------------------------------------

  connectedCallback() {
    if (this.#els === null) this.#build();
    this.#applyAttributes();
    this.#watchTheme();
    document.addEventListener("visibilitychange", this.#onVisibility);
    if (this.#handle !== null || this.#loading) return;
    this.#els.frame.dataset.state = "loading";
    this.#intersection = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        this.#intersection?.disconnect();
        this.#intersection = null;
        this.#load();
      },
      { rootMargin: "200px" },
    );
    this.#intersection.observe(this);
  }

  disconnectedCallback() {
    this.#intersection?.disconnect();
    this.#intersection = null;
    this.#resize?.disconnect();
    this.#resize = null;
    this.#mutation?.disconnect();
    this.#mutation = null;
    this.#media?.removeEventListener("change", this.#onTheme);
    this.#resolution?.removeEventListener("change", this.#onResolution);
    this.#resolution = null;
    document.removeEventListener("visibilitychange", this.#onVisibility);
    this.#deactivateWheel();
    if (this.#raf !== 0) {
      cancelAnimationFrame(this.#raf);
      this.#raf = 0;
    }
    this.#dirty = false;
    this.#pointers.clear();
    if (this.#handle !== null) {
      try {
        this.#handle.release();
      } catch (error) {
        console.warn("ironlab-figure: release failed", error);
      }
      this.#handle = null;
    }
    // A reconnected element opens the figure again, so it starts from the fallback with its toolbar hidden.
    this.#rendered = false;
    this.#sized = false;
    this.#backdrop = null;
    if (this.#els !== null) {
      this.#els.figurebar.hidden = true;
      this.#els.frame.dataset.state = "loading";
    }
  }

  attributeChangedCallback(name) {
    if (this.#els === null) return;
    switch (name) {
      case "theme":
        this.#pushBackdrop(true);
        break;
      case "alt":
        this.#els.canvas.setAttribute("aria-label", this.alt);
        break;
      case "toolbar":
        this.#els.figurebar.hidden = !this.#rendered || this.toolbar === "hidden";
        break;
      default:
    }
  }

  #applyAttributes() {
    this.#els.canvas.setAttribute("aria-label", this.alt);
    this.#els.figurebar.hidden = !this.#rendered || this.toolbar === "hidden";
  }

  // ----- building the shadow tree ------------------------------------------------------------------------------------

  #build() {
    const root = this.#root;
    adoptStyles(root);
    root.innerHTML = FIGUREBAR_HTML + FRAME_HTML;
    const q = (selector) => root.querySelector(selector);
    this.#els = {
      figurebar: q('[part~="figurebar"]'),
      frame: q('[part~="frame"]'),
      canvas: q("canvas"),
      band: q('[part~="band"]'),
      overlay: q('[part~="overlay"]'),
      ring: q('[part~="ring"]'),
      outline: q('[part~="outline"]'),
      datatip: q('[part~="datatip"]'),
      hint: q('[part~="hint"]'),
      status: q('[part~="status"]'),
      probe: q('[part~="probe"]'),
      problems: q('[part~="problems"]'),
      problemList: q('[part~="problem-list"]'),
      problemsSeparator: q('[data-for="problems"]'),
      tools: {
        pan: q('[data-action="pan"]'),
        zoom: q('[data-action="zoom"]'),
        rotate: q('[data-action="rotate"]'),
      },
      undo: q('[data-action="undo"]'),
      redo: q('[data-action="redo"]'),
      problemsButton: q('[data-action="problems"]'),
      export: q('[data-action="export"]'),
    };

    const { canvas, frame, figurebar, problems, problemsButton } = this.#els;

    canvas.addEventListener("pointerdown", this.#onPointerDown);
    canvas.addEventListener("pointermove", this.#onPointerMove);
    canvas.addEventListener("pointerup", this.#onPointerUp);
    canvas.addEventListener("pointercancel", this.#onPointerCancel);
    canvas.addEventListener("pointerleave", this.#onPointerLeave);
    // Clicks and double clicks are not forwarded: the wasm's recogniser decides both from the presses and releases
    // it is given, by the native viewer's own rules, so a forwarded DOM click would be a second one.
    canvas.addEventListener("contextmenu", (event) => {
      // A right click is not a gesture the figure knows; the page's menu is left alone unless a drag is in flight.
      if (this.#pointers.size > 0) event.preventDefault();
    });

    // The wheel scrolls the page until the reader activates the figure, so the listener must be able to prevent
    // the default only once that has happened.
    frame.addEventListener("wheel", this.#onWheel, { passive: false });
    frame.addEventListener("pointerdown", this.#activateWheel);
    frame.addEventListener("click", this.#activateWheel);

    figurebar.addEventListener("click", this.#onToolbarClick);
    root.addEventListener("keydown", this.#onKeyDown);
    this.addEventListener("focusout", this.#onFocusOut);

    if (typeof problems.togglePopover !== "function") {
      // No popover support: the list is shown and hidden by hand beneath the button.
      problems.removeAttribute("popover");
      problems.hidden = true;
    }
    problems.addEventListener("beforetoggle", (event) => {
      if (event.newState === "open") this.#fillProblems();
    });
    problems.addEventListener("toggle", (event) => {
      const open = event.newState === "open";
      problemsButton.setAttribute("aria-expanded", String(open));
      if (open) this.#placeProblems();
    });
  }

  // ----- loading -----------------------------------------------------------------------------------------------------

  async #load() {
    if (this.#loading || this.#handle !== null) return;
    this.#loading = true;
    try {
      const src = this.getAttribute("src");
      if (!src) throw new Error("The src attribute is missing.");
      const url = new URL(src, document.baseURI);
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`${url.pathname} could not be fetched (${response.status} ${response.statusText}).`);
      }
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (!this.isConnected) return;

      let gpu;
      try {
        gpu = await session();
      } catch (error) {
        this.#fallback(STATUS_NO_GPU, error);
        return;
      }
      if (!this.isConnected) return;

      const handle = await gpu.open(this.#els.canvas, bytes, this.format);
      if (!this.isConnected) {
        handle.release();
        return;
      }
      this.#handle = handle;
      this.#fitFrame();
      this.#els.tools.rotate.hidden = !handle.has_3d();
      this.#chrome(handle.can_undo(), handle.can_redo(), handle.problems().length);
      this.#pushBackdrop(true);
      this.#observeSize();
    } catch (error) {
      this.#fallback(`The figure could not be loaded: ${messageOf(error)}`, error);
    } finally {
      this.#loading = false;
    }
  }

  /** Keeps the fallback content, says why in the status line, and tells the page. */
  #fallback(message, error) {
    this.#els.frame.dataset.state = "fallback";
    this.#status(message);
    console.warn(`ironlab-figure: ${message}`, error);
    this.#settle(false, error instanceof Error ? error : new Error(message));
    this.dispatchEvent(new CustomEvent("ironlab-error", { bubbles: true, composed: true, detail: { message } }));
  }

  #settle(ok, value) {
    if (this.#settled) return;
    this.#settled = true;
    if (ok) this.#resolveReady(value);
    else this.#rejectReady(value);
  }

  /** Gives the frame the figure's aspect ratio, unless the `height` attribute fixes its height instead. */
  #fitFrame() {
    const { frame } = this.#els;
    const height = this.getAttribute("height");
    if (height) {
      frame.style.height = /^\d+(\.\d+)?$/.test(height) ? `${height}px` : height;
      frame.style.aspectRatio = "";
      return;
    }
    frame.style.height = "";
    const size = this.#handle.size_pt();
    if (size[0] > 0 && size[1] > 0) frame.style.aspectRatio = `${size[0]} / ${size[1]}`;
  }

  #observeSize() {
    const { canvas } = this.#els;
    this.#resize = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1];
      const device = entry.devicePixelContentBoxSize?.[0];
      if (device) {
        this.#applySize(device.inlineSize, device.blockSize);
      } else {
        const ratio = window.devicePixelRatio || 1;
        this.#applySize(Math.round(entry.contentRect.width * ratio), Math.round(entry.contentRect.height * ratio));
      }
    });
    try {
      this.#resize.observe(canvas, { box: "device-pixel-content-box" });
    } catch {
      this.#resize.observe(canvas);
    }
    // A change of device pixel ratio without a change of CSS size (browser zoom, a move between screens) does not
    // fire the observer when it is measuring content boxes, so it is watched separately.
    this.#watchResolution();
  }

  #watchResolution() {
    this.#resolution?.removeEventListener("change", this.#onResolution);
    this.#resolution = matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
    this.#resolution.addEventListener("change", this.#onResolution);
  }

  #onResolution = () => {
    const { canvas } = this.#els;
    const rect = canvas.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    this.#applySize(Math.round(rect.width * ratio), Math.round(rect.height * ratio));
    this.#watchResolution();
  };

  #applySize(width, height) {
    if (!this.#handle || width <= 0 || height <= 0) return;
    const { canvas } = this.#els;
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
    try {
      this.#handle.resize(width, height, window.devicePixelRatio || 1);
    } catch (error) {
      this.#fail(error);
      return;
    }
    this.#sized = true;
    this.#markDirty();
  }

  // ----- rendering on demand -----------------------------------------------------------------------------------------

  #markDirty() {
    this.#dirty = true;
    if (this.#raf === 0) this.#raf = requestAnimationFrame(this.#frame);
  }

  #frame = () => {
    this.#raf = 0;
    if (!this.#dirty || this.#handle === null) return;
    // A frame cannot be drawn until the observer has sized the canvas; the dirty flag is kept, and the first size
    // schedules the frame again.
    if (!this.#sized) return;
    this.#dirty = false;
    const { canvas } = this.#els;
    if (canvas.width === 0 || canvas.height === 0) return;
    try {
      this.#handle.render();
    } catch (error) {
      this.#fail(error);
      return;
    }
    if (!this.#rendered) this.#firstRender();
  };

  #firstRender() {
    this.#rendered = true;
    const { frame, figurebar } = this.#els;
    frame.dataset.state = "ready";
    figurebar.hidden = this.toolbar === "hidden";
    this.#status("");
    this.#settle(true, this.#handle);
    this.dispatchEvent(
      new CustomEvent("ironlab-ready", { bubbles: true, composed: true, detail: { handle: this.#handle } }),
    );
  }

  /** A GPU failure after the figure opened: the last frame stays, the status says what happened, nothing retries. */
  #fail(error) {
    const message = `The figure could not be drawn: ${messageOf(error)}`;
    if (!this.#rendered) this.#els.frame.dataset.state = "fallback";
    this.#status(message);
    console.error("ironlab-figure:", error);
    this.#settle(false, error instanceof Error ? error : new Error(message));
    this.dispatchEvent(new CustomEvent("ironlab-error", { bubbles: true, composed: true, detail: { message } }));
  }

  #onVisibility = () => {
    if (document.visibilityState === "visible" && this.#handle !== null) this.#markDirty();
  };

  // ----- outcomes: what the chrome shows -----------------------------------------------------------------------------

  #apply(outcome) {
    if (!outcome) return;
    if (outcome.redraw) {
      // `redraw` is whether the input changed the figure, so it is exactly when the page is told of a change.
      this.#markDirty();
      this.#changed();
    }
    const { canvas, band } = this.#els;
    canvas.style.cursor = outcome.cursor || "";
    if (outcome.rubber_band) {
      const [x, y, w, h] = outcome.rubber_band;
      band.style.left = `${x}px`;
      band.style.top = `${y}px`;
      band.style.width = `${w}px`;
      band.style.height = `${h}px`;
      band.hidden = false;
    } else {
      band.hidden = true;
    }
    this.#datatip(outcome.datatip);
    this.#chrome(outcome.can_undo, outcome.can_redo, outcome.problem_count);
  }

  #chrome(canUndo, canRedo, problemCount) {
    const { tools, undo, redo, problemsButton, problemsSeparator, problems } = this.#els;
    const tool = this.#handle ? this.#handle.tool() : "pan";
    for (const [name, control] of Object.entries(tools)) {
      control.setAttribute("aria-pressed", String(name === tool));
    }
    undo.disabled = !canUndo;
    redo.disabled = !canRedo;
    const count = Number(problemCount) || 0;
    problemsButton.hidden = count === 0;
    problemsSeparator.hidden = count === 0;
    problemsButton.querySelector('[part~="caption"]').textContent = problemsCaption(count);
    if (count === 0) this.#closeProblems();
  }

  #problemsOpen() {
    const { problems } = this.#els;
    if (problems.hasAttribute("popover")) return problems.matches(":popover-open");
    return !problems.hidden;
  }

  #closeProblems() {
    const { problems, problemsButton } = this.#els;
    if (!this.#problemsOpen()) return;
    if (problems.hasAttribute("popover")) problems.hidePopover();
    else problems.hidden = true;
    problemsButton.setAttribute("aria-expanded", "false");
  }

  #datatip(tip) {
    const { datatip, overlay, ring, outline, frame } = this.#els;
    if (!tip) {
      datatip.hidden = true;
      overlay.hidden = true;
      return;
    }
    if (datatip.textContent !== tip.text) datatip.textContent = tip.text;
    datatip.hidden = false;
    // The tooltip sits off the anchor's lower right, and flips to the upper left where it would leave the frame.
    const [ax, ay] = tip.anchor;
    const width = datatip.offsetWidth;
    const height = datatip.offsetHeight;
    let left = ax + DATATIP_OFFSET;
    let top = ay + DATATIP_OFFSET;
    if (left + width > frame.clientWidth - 4) left = Math.max(4, ax - DATATIP_OFFSET - width);
    if (top + height > frame.clientHeight - 4) top = Math.max(4, ay - DATATIP_OFFSET - height);
    datatip.style.transform = `translate(${left}px, ${top}px)`;

    const marker = tip.marker;
    if (marker && marker.kind === "ring") {
      ring.setAttribute("cx", marker.cx);
      ring.setAttribute("cy", marker.cy);
      ring.setAttribute("r", marker.r);
      ring.hidden = false;
      outline.hidden = true;
      overlay.hidden = false;
    } else if (marker && marker.kind === "outline") {
      outline.setAttribute("points", marker.points.map(([x, y]) => `${x},${y}`).join(" "));
      outline.hidden = false;
      ring.hidden = true;
      overlay.hidden = false;
    } else {
      overlay.hidden = true;
    }
  }

  #status(text) {
    const { status } = this.#els;
    status.textContent = text;
    status.hidden = !text;
  }

  #changed() {
    this.dispatchEvent(new CustomEvent("ironlab-change", { bubbles: true, composed: true }));
  }

  // ----- pointer input -----------------------------------------------------------------------------------------------

  #point(event) {
    const rect = this.#els.canvas.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  }

  #onPointerDown = (event) => {
    if (this.#handle === null) return;
    if (event.pointerType === "mouse" && event.button !== 0) return;
    // Stops the browser selecting text around the figure while the pointer is dragged.
    event.preventDefault();
    this.#els.canvas.focus({ preventScroll: true });
    const point = this.#point(event);
    this.#els.canvas.setPointerCapture(event.pointerId);
    if (this.#pointers.size === 0) {
      this.#pointers.set(event.pointerId, point);
      this.#apply(this.#handle.pointer_down(point.x, point.y));
    } else if (this.#pointers.size === 1) {
      // A second finger turns the press into a pinch: the first finger's press is cancelled, which ends a drag where
      // it stands and is never a click, and from here on the pair is reported as a zoom about its midpoint.
      const [first] = this.#pointers.values();
      this.#apply(this.#handle.pointer_cancel());
      this.#pointers.set(event.pointerId, point);
      this.#pinchDistance = Math.hypot(first.x - point.x, first.y - point.y);
    }
    // A third pointer is ignored.
  };

  #onPointerMove = (event) => {
    if (this.#handle === null) return;
    const point = this.#point(event);
    if (this.#pointers.has(event.pointerId)) this.#pointers.set(event.pointerId, point);
    if (this.#pointers.size >= 2) {
      const [a, b] = this.#pointers.values();
      const distance = Math.hypot(a.x - b.x, a.y - b.y);
      if (this.#pinchDistance > 0 && distance > 0) {
        const ratio = distance / this.#pinchDistance;
        this.#apply(this.#handle.wheel((a.x + b.x) / 2, (a.y + b.y) / 2, 0, ratio));
      }
      this.#pinchDistance = distance;
      return;
    }
    this.#apply(this.#handle.pointer_move(point.x, point.y, event.buttons));
  };

  #onPointerUp = (event) => {
    if (this.#handle === null) return;
    const { canvas } = this.#els;
    if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
    const wasPinch = this.#pointers.size >= 2;
    const tracked = this.#pointers.delete(event.pointerId);
    if (!tracked) return;
    if (wasPinch) {
      // The pinch ends when either finger lifts; the other is forgotten so that it is not read as a drag.
      this.#pointers.clear();
      this.#pinchDistance = 0;
      return;
    }
    const point = this.#point(event);
    this.#apply(this.#handle.pointer_up(point.x, point.y));
  };

  /** The browser cancelled the pointer (a gesture the platform took over, a lost capture): the press is not a click. */
  #onPointerCancel = (event) => {
    if (this.#handle === null) return;
    const { canvas } = this.#els;
    if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
    const wasPinch = this.#pointers.size >= 2;
    if (!this.#pointers.delete(event.pointerId)) return;
    if (wasPinch) {
      this.#pointers.clear();
      this.#pinchDistance = 0;
      return;
    }
    this.#apply(this.#handle.pointer_cancel());
  };

  #onPointerLeave = () => {
    if (this.#handle === null) return;
    this.#apply(this.#handle.pointer_leave());
    this.#datatip(null);
  };

  // ----- the wheel: scrolls the page until the figure is activated ---------------------------------------------------

  #onWheel = (event) => {
    if (!this.#wheelActive || this.#handle === null) return;
    event.preventDefault();
    const point = this.#point(event);
    // A trackpad pinch reaches the page as a wheel event with the Control key set; it is passed as a pinch ratio in
    // the way egui's own web backend reads it, so that a pinch on the web feels as it does in the native viewer.
    if (event.ctrlKey && event.deltaMode === 0) {
      this.#apply(this.#handle.wheel(point.x, point.y, 0, Math.exp(-event.deltaY / 200)));
    } else {
      const scale = event.deltaMode === 1 ? LINE_HEIGHT_PX : event.deltaMode === 2 ? window.innerHeight : 1;
      this.#apply(this.#handle.wheel(point.x, point.y, event.deltaY * scale, 1.0));
    }
  };

  #activateWheel = () => {
    if (this.#wheelActive || this.#handle === null) return;
    this.#wheelActive = true;
    this.#els.frame.dataset.active = "true";
    document.addEventListener("pointerdown", this.#onDocumentPointerDown, true);
  };

  #deactivateWheel = () => {
    if (!this.#wheelActive) return;
    this.#wheelActive = false;
    if (this.#els) this.#els.frame.dataset.active = "false";
    document.removeEventListener("pointerdown", this.#onDocumentPointerDown, true);
  };

  #onDocumentPointerDown = (event) => {
    if (!event.composedPath().includes(this)) this.#deactivateWheel();
  };

  #onFocusOut = (event) => {
    const next = event.relatedTarget;
    if (next && (this.contains(next) || this.#root.contains(next))) return;
    this.#deactivateWheel();
  };

  // ----- keyboard ----------------------------------------------------------------------------------------------------

  #onKeyDown = (event) => {
    if (event.key === "Escape") {
      this.#closeProblems();
      this.#deactivateWheel();
      return;
    }
    if (this.#handle === null || event.altKey) return;
    const command = event.metaKey || event.ctrlKey;
    if (!command && !event.shiftKey && (event.key === "r" || event.key === "R")) {
      event.preventDefault();
      this.refit();
    } else if (command && (event.key === "z" || event.key === "Z")) {
      event.preventDefault();
      if (event.shiftKey) this.#redo();
      else this.#undo();
    }
  };

  #undo() {
    if (this.#handle === null) return;
    this.#apply(this.#handle.undo());
  }

  #redo() {
    if (this.#handle === null) return;
    this.#apply(this.#handle.redo());
  }

  // ----- the figurebar -----------------------------------------------------------------------------------------------

  #onToolbarClick = (event) => {
    const control = event.target.closest("button[data-action]");
    if (!control || control.disabled || this.#handle === null) return;
    const action = control.dataset.action;
    switch (action) {
      case "pan":
      case "zoom":
      case "rotate":
        this.#apply(this.#handle.set_tool(action));
        this.#chrome(this.#handle.can_undo(), this.#handle.can_redo(), this.#handle.problems().length);
        this.#els.canvas.style.cursor = CURSORS[action];
        break;
      case "refit":
        this.refit();
        break;
      case "undo":
        this.#undo();
        break;
      case "redo":
        this.#redo();
        break;
      case "problems":
        if (!this.#els.problems.hasAttribute("popover")) this.#toggleProblemsByHand();
        break;
      case "save":
        try {
          this.save();
        } catch (error) {
          this.#status(`The figure could not be saved: ${messageOf(error)}`);
          console.error("ironlab-figure: save failed", error);
        }
        break;
      case "export":
        this.exportPdf().catch(() => {});
        break;
      default:
    }
  };

  #fillProblems() {
    const { problemList } = this.#els;
    problemList.replaceChildren();
    if (this.#handle === null) return;
    for (const problem of this.#handle.problems()) {
      const item = document.createElement("li");
      item.setAttribute("part", "problem");
      const subject = document.createElement("strong");
      subject.setAttribute("part", "problem-subject");
      subject.textContent = problem.subject;
      const detail = document.createElement("div");
      detail.setAttribute("part", "problem-detail");
      detail.textContent = problem.detail;
      const explanation = document.createElement("div");
      explanation.setAttribute("part", "problem-explanation");
      explanation.textContent = problem.explanation;
      item.append(subject, detail, explanation);
      problemList.append(item);
    }
  }

  /** Places the open popover beneath the problems button, right-aligned with it and kept within the viewport. */
  #placeProblems() {
    const { problems, problemsButton } = this.#els;
    const anchor = problemsButton.getBoundingClientRect();
    const width = problems.offsetWidth;
    let left = anchor.right - width;
    left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
    let top = anchor.bottom + 4;
    if (top + problems.offsetHeight > window.innerHeight - 8) top = Math.max(8, anchor.top - problems.offsetHeight - 4);
    problems.style.left = `${left}px`;
    problems.style.top = `${top}px`;
  }

  #toggleProblemsByHand() {
    const { problems, problemsButton } = this.#els;
    const open = problems.hidden;
    if (open) this.#fillProblems();
    problems.hidden = !open;
    problemsButton.setAttribute("aria-expanded", String(open));
    if (open) this.#placeProblems();
  }

  // ----- theming -----------------------------------------------------------------------------------------------------

  #watchTheme() {
    this.#media ??= matchMedia("(prefers-color-scheme: dark)");
    this.#media.addEventListener("change", this.#onTheme);
    // The documentation theme toggles `data-md-color-scheme` on <body>; other pages toggle a class or attribute on
    // <html>. Either changes what the tokens resolve to, so the backdrop is read again after either.
    this.#mutation ??= new MutationObserver(this.#onTheme);
    this.#mutation.observe(document.documentElement, { attributes: true });
    if (document.body) this.#mutation.observe(document.body, { attributes: true });
  }

  #onTheme = () => {
    this.#pushBackdrop(false);
  };

  /** Reads `--ironlab-canvas-bg` as the element resolves it and gives it to the wasm as the clear colour. */
  #pushBackdrop(force) {
    if (this.#handle === null) return;
    const value = getComputedStyle(this).getPropertyValue("--ironlab-canvas-bg").trim();
    if (!value) return;
    const { probe } = this.#els;
    probe.style.color = "";
    probe.style.color = value;
    const rgb = parseColour(getComputedStyle(probe).color);
    if (!rgb) return;
    const last = this.#backdrop;
    if (!force && last && last[0] === rgb[0] && last[1] === rgb[1] && last[2] === rgb[2]) return;
    this.#backdrop = rgb;
    this.#handle.set_backdrop(rgb[0], rgb[1], rgb[2]);
    this.#markDirty();
  }
}

IronlabFigure.define();

export default IronlabFigure;
