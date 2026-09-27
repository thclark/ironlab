# Embedding IronLAB figures in a web page

An IronLAB figure saved to a `.fig` or `.json` file can be shown in any web page as an interactive figure: it can be panned, zoomed and rotated, its data can be read with the pointer, and it can be saved or exported to PDF from the page, exactly as in the desktop viewer. This directory holds everything a page needs.

## The files

Copy these four files to one directory of your site, beside your figure files:

| File | What it is |
| --- | --- |
| `ironlab.js` | The `<ironlab-figure>` element, as an ES module. |
| `ironlab_core.js` | The JavaScript glue of the wasm module, which `ironlab.js` imports. |
| `ironlab_core_bg.wasm` | The figure renderer, compiled to WebAssembly. |
| `ironlab.css` | The light-DOM defaults: the element's place in the page and the colour tokens it draws from. |

Load the module and the stylesheet once per page, from wherever you put them:

```html
<link rel="stylesheet" href="figures/ironlab.css" />
<script type="module" src="figures/ironlab.js"></script>
```

The module defines the element when it loads. Loading it twice, or from two pages of the same site, is harmless.

## The element

```html
<ironlab-figure src="figures/damped-oscillator.fig" alt="A damped oscillator">
  <a href="figures/damped-oscillator.pdf">
    <img src="figures/damped-oscillator.png" alt="A damped oscillator" />
  </a>
</ironlab-figure>
```

The content inside the element is the fallback. It is shown until the figure has been drawn for the first time, and it stays, with a line beneath it saying why, when the browser cannot draw the figure. A linked still image is the usual fallback, because it gives every reader the figure and a PDF of it; the element works without any fallback too.

| Attribute | Meaning |
| --- | --- |
| `src` | The figure file, resolved against the page's base URL. Required. |
| `format` | `fig` (Protocol Buffers) or `json`. The default is taken from the last extension of `src`: `.fig` gives `fig`, `.json` gives `json`. |
| `name` | The name used for downloaded files, as `name.fig`, `name.json` or `name.pdf`. The default is the file name of `src` without its `.fig`, `.json` or `.fig.json` extension. |
| `alt` | The accessible name of the canvas. The default is `name`. |
| `toolbar` | `auto` (the default) shows the toolbar above the figure; `hidden` shows none. |
| `height` | Fixes the frame's height, as a number of CSS pixels or any CSS length. Without it, the frame takes the page's full width and the figure's own aspect ratio. |
| `theme` | `auto` (the default) follows the reader's colour scheme; `light` and `dark` fix the chrome's colours. A page can instead set the `--ironlab-*` tokens listed in `ironlab.css` on the element, which overrides all three. |

Figures are created lazily: a figure begins loading when it comes within 200 pixels of the viewport, and nothing is fetched for a figure far down the page until the reader scrolls towards it. Browsers allow roughly sixteen WebGL canvases on a page at once, and a page with more figures than that should expect the earliest to lose their context; a page of a dozen figures is safe.

## Reading the figure

The toolbar has the desktop viewer's controls: **Pan**, **Zoom** and **Rotate** decide what dragging does (Rotate appears only when the figure has a three-dimensional axes); **Refit** restores the limits and views of every axes; **Undo** and **Redo** step through the changes; **Save figure…** downloads the figure as shown, in the encoding it was loaded from; and **Export PDF…** downloads a PDF of the figure as shown. When the figure has problems, an indicator naming their number appears at the right, and clicking it lists them.

Resting the pointer near a data point rings it and shows its name, coordinates and index; resting it over an image outlines the pixel and shows what it holds. Double-clicking an axes restores that axes alone.

**The wheel.** Scrolling over a figure scrolls the page, as a reader expects of an article, until the reader clicks or touches the figure. From then on the wheel zooms the axes under the pointer, and a trackpad pinch does the same, until the reader presses Escape, clicks elsewhere on the page or moves focus out of the figure. A hint over the figure says so while the wheel is not yet taken.

**Keyboard.** When the figure or one of its buttons has focus: **R** refits, **⌘Z** or **Ctrl+Z** undoes, and **⌘⇧Z** or **Ctrl+Shift+Z** redoes. **Escape** closes the problems list and gives the wheel back to the page.

## The JavaScript API

Each element exposes the figure to scripts on the page:

```js
const figure = document.querySelector("ironlab-figure");
const handle = await figure.ready; // the wasm FigureHandle, once the figure has been drawn
figure.handle;                     // the same handle, or null before it exists
figure.refit();                    // as the Refit button
figure.save();                     // downloads name.fig or name.json
await figure.exportPdf();          // downloads name.pdf and resolves with the exporter's warnings; rejects if it fails
await figure.session;              // the page's one wasm Session; session.backend() is "webgpu" or "webgl2"
```

The module also exports `session()`, the promise of that one session, and `version()`, the version of the wasm module.

Four events bubble from the element, and all of them cross the shadow boundary, so a listener on `document` hears every figure on the page:

| Event | When |
| --- | --- |
| `ironlab-ready` | The figure has been drawn for the first time. `detail.handle` is the handle. |
| `ironlab-error` | The figure could not be loaded or drawn. `detail.message` says why, and the fallback content is shown. |
| `ironlab-change` | An input changed what the figure shows: a drag, the wheel, a click on a legend entry, a double click, Undo, Redo or Refit. It is dispatched exactly when the figure changed, never for an input that changed nothing. |
| `ironlab-export` | A PDF was exported. `detail.warnings` lists the exporter's warnings as `{ subject, detail, explanation }`: what was drawn as an image rather than as vectors, and which three-dimensional axes could not be verified against the screen. The status line names the first, and the list is logged to the console. |

## Requirements and hosting

The renderer needs WebGPU or WebGL2. Every current desktop and mobile browser has WebGL2; where neither is available, or the graphics driver is blocked, every figure on the page shows its fallback content with the line "This figure needs WebGPU or WebGL2 to be interactive." A module that fails to load or initialise is reported differently, with the line "The figure engine could not be started" followed by the browser's reason; that is a fault of the bundle or of its hosting, not of the reader's browser.

- The server must send `ironlab_core_bg.wasm` with the media type `application/wasm`, which most servers do by extension. Python's `http.server`, nginx, Apache and every static host do.
- The `.fig` files are binary and should be served as `application/octet-stream` or `application/x-protobuf`; the `.json` files as `application/json`. Neither needs a particular type for the element to read them.
- No cross-origin isolation headers are needed: the module uses no shared memory and no worker.
- The page must be served over HTTP or HTTPS, not opened from the file system, because ES modules and `fetch` do not work from `file:` URLs.

## Licence

These files are part of IronLAB and are licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`). A figure shown with them, and the page that shows it, is your own work and is not affected.

If this conflicts with your use case, please raise an issue on the IronLAB repository and describe what you're trying to do. For commercial projects we can simply arrange a one-off license fee and for non-profit / academic efforts we may waive the fee.
