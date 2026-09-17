# ADR 0002: egui viewer on Rerun-family crates

**Status:** Accepted

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0006](0006-interaction-mutates-the-ir.md), [ADR 0007](0007-mvp-scope.md)

## Context

IronLAB needs a cross-platform desktop viewer. The viewer has two parts with different requirements. The shell (toolbar, tabs, dialogs and, in future, a property editor) is conventional widget work. The canvas is one large custom-drawn region that must stay responsive while panning and zooming large data sets, and it will need direct GPU access for picking and for images and surfaces.

The [background discussion](../background/concept-discussion.md) compared egui with eframe, iced, Slint, Xilem, Dioxus (with a system webview or with the Blitz renderer) and webview-based shells. The deciding considerations were:

- the canvas needs an escape hatch to raw wgpu rendering inside the user interface;
- Rerun's viewer, which uses egui on wgpu, is working evidence that this stack can carry a scientific viewer of this shape, and its visual style is appropriate for a scientific application;
- egui releases breaking changes in each minor version, and every third-party egui widget crate must be upgraded in step, so each additional maintainer in the dependency set adds upgrade risk;
- a component library for the shell is a small part of the work compared with the figure model, rendering and export, and should not decide the stack;
- a webview shell brings a JavaScript runtime and platform webview dependencies, of which WebKitGTK is unreliable on conservative Linux systems such as cluster login nodes.

## Decision

The viewer is built with egui and eframe on the wgpu backend, and its user-interface dependencies are limited to crates maintained within the Rerun family, which are released together:

- `egui` and `eframe` for the application and widgets;
- `egui_tiles` for tabs, with one tab per figure;
- `egui_kittest` for tests that drive the application.

The only dependency outside that family is `rfd`, a small and stable crate for native file dialogs, used by Export PDF. Rerun's internal design-system crate `re_ui` is not a dependency, because it is versioned with Rerun rather than with egui and carries no stability promise; parts of it may be copied and adapted instead. The table widget `egui_table` is not used, because data inspection in IronLAB means identifying points on the canvas, not tabulating data.

The workspace declares these crates with version requirements that each admit a single minor version, and upgrades them together and deliberately.

## Consequences

- Upgrading the user interface is one decision per egui release rather than one per widget crate, which removes most of the version-lockstep risk of the egui ecosystem.
- The canvas can later be replaced by custom wgpu pipelines through egui's paint callbacks without changing the shell, as described in [ADR 0003](0003-shared-scene-compiler-and-display-list.md).
- eframe also compiles to WebAssembly, which keeps a browser viewer possible without a second user-interface implementation.
- The project depends substantially on the direction of one company and one principal maintainer. The licences (MIT and Apache 2.0) permit a fork if that direction diverges, and the retained figure model of [ADR 0001](0001-retained-figure-ir-and-json-schema.md) keeps the viewer replaceable: a different shell would reimplement the chrome, not the engine.
- The workspace requires a recent Rust toolchain (1.95 or later), because egui 0.36 and its companion crates do.
- Styling is limited to egui's `Style` structure. This is sufficient for a viewer whose chrome is meant to be quiet, but a richly styled interface would be costly.
