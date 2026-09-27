# ironlab-web

The browser host of [IronLAB](https://ironlab.org), an interactive plotting tool for scientific computing in Rust.

This crate compiles the figure canvas, [`ironlab-canvas`](../ironlab-canvas), to WebAssembly and exposes it to JavaScript through `wasm-bindgen`, so that a web page shows a `.fig` or `.json` figure on a `<canvas>` and pans, zooms, rotates, reads, saves and exports it exactly as the desktop viewer does, through the same engine. The page-side half is the `<ironlab-figure>` custom element in [`www/ironlab.js`](www/ironlab.js), which fetches the file, owns the canvas and the toolbar, and forwards every pointer event to the module.

The crate is not published: a page consumes the bundle that `scripts/build-web.sh` assembles, which a release attaches as `ironlab-web-<version>.tar.gz` and the documentation site serves at `ironlab.org/embed/`. The guide to embedding a figure in a page, which ships inside the bundle, is [`www/README.md`](www/README.md).

## What the module exposes

Two classes, documented in the crate's own Rust documentation (`cargo doc --no-deps -p ironlab-web`):

- `Session`, created once per page. `Session.create()` finds the graphics backend the browser has, WebGPU where it is available and WebGL 2 otherwise, and rejects naming the missing capability when it has neither. `Session.headless()` has no graphics device: figures opened on it can be driven, saved and exported but not drawn. Every session shares one text engine, so the bundled fonts are parsed once.
- `FigureHandle`, one open figure. Its input methods (`pointer_down`, `pointer_move`, `pointer_up`, `pointer_cancel`, `pointer_leave`, `wheel`, `set_tool`, `undo`, `redo`, `refit`) take CSS pixels from the canvas's top-left corner and each returns an `Outcome`, a plain object of `{ redraw, cursor, rubber_band, datatip, can_undo, can_redo, problem_count }` that the page draws its chrome from. `render()` draws a frame; `save()` returns the figure as shown in the encoding it was opened from; `export_pdf()` resolves to `{ bytes, warnings }`. Clicks and double clicks are decided by the module's own pointer recogniser, which applies egui's rules, so the page forwards neither.

## Building the bundle

```sh
scripts/build-web.sh
```

The script compiles the crate for `wasm32-unknown-unknown` in the size-tuned `web` profile of the root `Cargo.toml`, generates the bindings with `wasm-bindgen`, shrinks the module with `wasm-opt`, inlines the shadow stylesheet into the loader, and assembles `dist/`. It checks for every tool it needs and prints the command that installs a missing one: the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` at exactly the version of the `wasm-bindgen` crate in `Cargo.lock`, `binaryen` for `wasm-opt`, `jq` and `python3`. `--no-opt` skips `wasm-opt` for a quick local build, and `--out DIR` chooses the output directory.

## Running the tests

The plain Rust side (the outcome a page reads, and the stem a download is named by) is tested natively:

```sh
cargo test -p ironlab-web
```

The `wasm_bindgen` surface is tested in headless Chrome by `wasm-bindgen-test-runner`, which `.cargo/config.toml` names as the runner for the wasm32 target and which reads the browser's arguments from [`webdriver.json`](webdriver.json) in this directory. `chromedriver` must be installed at the version of the installed Chrome; name it with `CHROMEDRIVER` when it is not on `PATH`:

```sh
CHROMEDRIVER=~/.cargo/bin/chromedriver WASM_BINDGEN_TEST_TIMEOUT=180 \
  cargo test --target wasm32-unknown-unknown -p ironlab-web
```

Every test but one runs without a graphics backend, through `Session.headless()`; the one that renders on a canvas skips itself with a console warning when the browser has neither WebGPU nor WebGL 2. The timeout is per test, in seconds, and the default of twenty is too short for an unoptimised build that typesets the deepest admitted mathematics.

## Trying the element

[`www/index.html`](www/index.html) shows the three test fixtures with the element. It loads the bundle from `dist/` beside `www/`, so build into this directory and serve it:

```sh
scripts/build-web.sh --out crates/ironlab-web/dist
python3 -m http.server -d crates/ironlab-web 8000
```

and open <http://localhost:8000/www/index.html>. Both `dist/` directories are ignored by Git.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).

If this conflicts with your use case, please raise an issue on the IronLAB repository and describe what you're trying to do. For commercial projects we can simply arrange a one-off license fee and for non-profit / academic efforts we may waive the fee.
