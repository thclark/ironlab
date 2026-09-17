# ADR 0001: Retained figure IR and JSON Schema

**Status:** Accepted

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0005](0005-embedded-latex-math-with-latex-rust.md), [ADR 0006](0006-interaction-mutates-the-ir.md), [ADR 0007](0007-mvp-scope.md)

## Context

IronLAB aims to provide what MATLAB's figure engine provides (an interactive figure whose axes, plots and text are objects with properties that can be inspected and changed) together with what MATLAB does not provide: publication-quality vector output with typeset mathematics. It must also leave room for future front ends: a live connection over a socket, client libraries in other languages, and a browser viewer.

Most plotting libraries hold a figure only implicitly, as state scattered through the code that draws it. That works for one output format, but a second format, an interactive view that must agree with an export, or a remote client that must describe a figure, then requires rewriting the drawing code. MATLAB avoids this with handle graphics: every graphics object is a node with a stable handle in a retained tree, and both scripts and the user interface read and write its properties.

The [background discussion](../background/concept-discussion.md) considered this problem first and concluded that the transport (in-process, file or socket) matters less than what is transported.

## Decision

IronLAB is built around a retained intermediate representation (IR) of a figure, defined in the `ironlab-ir` crate.

- A figure is a serialisable tree of nodes (the figure, its axes and their artists). Each node has a `NodeId` that is unique within the figure and stable across saving and loading.
- Numeric data is held in a table of n-dimensional, row-major arrays keyed by `DataId`, and artists refer to arrays by identifier rather than containing them.
- The IR describes intent (axes limits, styles, text source, colormap names), not geometry, pixels or glyphs.
- The JSON serialisation of the IR is the `.fig.json` file format. Its JSON Schema, `schema/figure.schema.json`, is generated from the Rust types with `schemars`, and a test fails when the committed schema differs from the generated one. The Rust types, not the schema file, are the source of truth.
- Tagged variants are JSON objects with a `type` property, so that new variants can be added without changing existing ones. Missing values in arrays are NaN in memory and `null` in JSON.
- The file records a `schema_version`. Files load when their major and minor versions match the build's, and new artist variants or projections increment the minor version.
- The file records the provenance of the figure: the IronLAB version, the typesetter version and the fonts.
- Validation of relationships that the schema cannot express (array lengths, shapes, references, links and layout) is a separate step that returns structured errors and warnings, so that building a figure never panics on inconsistent input.

The schema is described entity by entity in the [figure schema reference](../reference/figure-schema.md).

## Consequences

- Every consumer of a figure (the viewer, the PDF exporter, the gallery, and in future any socket protocol or client library) works from one model, so adding a consumer does not change the others.
- A `.fig.json` file is a complete, reproducible description of a figure that can be archived with results and reopened in the viewer.
- JSON was chosen over a binary format for the MVP because it is readable and easy to debug. Large arrays are stored verbosely as a result; a binary encoding for bulk data (such as Apache Arrow) can be added later without changing the model, because data is already separated from structure by `DataId`.
- The schema is a compatibility surface. Changes to it require a version increment and, before implementation, a reviewed definition of the new data structures, as the project rules require.
- Generating the schema from the Rust types avoids maintaining two definitions, at the cost of tying the schema's form to what `schemars` produces.
