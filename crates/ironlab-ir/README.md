# ironlab-ir

The retained figure intermediate representation for [IronLAB](https://ironlab.org), an interactive plotting tool for scientific computing in Rust.

A figure is a serialisable tree of nodes — the figure, its axes and their artists — in which every node carries a stable identifier and every property that affects the drawing. Numeric arrays are held in a table beside the tree and referenced by identifier, so the same data can be shared. The crate also provides validation, identifier allocation, the linking of axis limits, and the typed edit and overlay machinery through which the viewer records a user's changes without mutating the figure it was given.

The Rust types in this crate are the single source of truth for the IronLAB figure format. The default file and transport format is Protocol Buffers (`.fig`); JSON (`.fig.json`) is a supported secondary format for debugging, other tools and simple web pages. Both describe the same figure, so converting between them loses nothing. The `.proto` files and the JSON Schema are generated from the Rust types as build artefacts rather than committed, which is what allows programs in other languages to read and write IronLAB figures.

## Where this crate sits

IronLAB is a Cargo workspace, and `ironlab-ir` is its foundation. It depends on no other IronLAB crate and has no knowledge of drawing: layout and geometry belong to `ironlab-scene`, and rendering to `ironlab-pdf` and `ironlab-viewer`. Every other crate in the workspace depends on it, directly or through the facade.

Most users should depend on the [`ironlab`](https://crates.io/crates/ironlab) facade crate, which re-exports these types as `ironlab::ir` alongside the figure-building API. Depend on `ironlab-ir` directly only when you need the figure model on its own, for instance to read, write or transform `.fig` files without building or drawing figures.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org). The [figure schema](https://ironlab.org/reference/figure-schema/) describes every entity of the model and both of its encodings, and the [architecture](https://ironlab.org/reference/architecture/) explains how the crates fit together. The source is at [github.com/thclark/ironlab](https://github.com/thclark/ironlab).

IronLAB is moving quickly and breaking changes are expected. Pin to an exact version.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).
