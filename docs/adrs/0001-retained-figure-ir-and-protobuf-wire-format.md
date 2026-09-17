# ADR 0001: Retained figure IR and Protocol Buffers wire format

**Status:** Accepted

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0005](0005-embedded-latex-math-with-latex-rust.md), [ADR 0006](0006-interaction-mutates-the-ir.md), [ADR 0007](0007-mvp-scope.md)

## Context

IronLAB aims to provide what MATLAB's figure engine provides (an interactive figure whose axes, plots and text are objects with properties that can be inspected and changed) together with what MATLAB does not provide: publication-quality vector output with typeset mathematics. It must also leave room for future front ends: a live connection over a socket, client libraries in other languages, and a browser viewer.

Most plotting libraries hold a figure only implicitly, as state scattered through the code that draws it. That works for one output format, but a second format, an interactive view that must agree with an export, or a remote client that must describe a figure, then requires rewriting the drawing code. MATLAB avoids this with handle graphics: every graphics object is a node with a stable handle in a retained tree, and both scripts and the user interface read and write its properties.

The [background discussion](../background/concept-discussion.md) considered this problem first and concluded that the transport (in-process, file or socket) matters less than what is transported. What is transported still needs an encoding. Scientific figures routinely hold arrays of millions of values, the same encoding will carry figures between processes and languages, and other languages need a schema from which to generate their own types.

## Decision

### A retained model

IronLAB is built around a retained intermediate representation (IR) of a figure, defined in the `ironlab-ir` crate.

- A figure is a tree of nodes (the figure, its axes and their artists). Each node has a `NodeId` that is unique within the figure and stable across saving and loading.
- Numeric data is held in a table of n-dimensional, row-major arrays keyed by `DataId`, and artists refer to arrays by identifier rather than containing them.
- The IR describes intent (axes limits, styles, text source, colormap names), not geometry, pixels or glyphs.
- The figure records a `schema_version`. A figure loads when its major and minor versions match the build's, and new artist variants or projections increment the minor version.
- The figure records its provenance: the IronLAB version, the typesetter version and the fonts.
- Validation of relationships that an encoding cannot express (array lengths, shapes, references, links and layout) is a separate step that returns structured errors and warnings, so that building a figure never panics on inconsistent input.

### The Rust types are the source of truth

The Rust types of `ironlab-ir` are the only definition of the model. Every schema that describes the model to other software is generated from them as a build artefact and is never committed, so no schema can drift from the code.

### Protocol Buffers is the default encoding

The default file format (`.fig`) and the default transport format are the Protocol Buffers encoding of the IR, in package `ironlab.ir.v0`.

- The `wire` module of `ironlab-ir` defines message types that mirror the domain types, and converts losslessly between the two. The messages are encoded and decoded with `prost`.
- Each wire type is declared once, with a macro that expands the declaration both into the Rust type with its `prost` attributes and into a description from which the `.proto` text is rendered. The field numbers, labels and types of the Rust type and of the generated schema therefore cannot diverge, and the macro has no syntax for a field without an explicit number.
- `cargo run -p ironlab-ir --bin generate-proto` writes one `.proto` file per module of `ironlab-ir`, together with a `buf.yaml`, to `target/ironlab-proto/`.
- Field 1 of the `Figure` message is `schema_version` in every version, so that a reader can check compatibility before decoding the rest of a file.
- CI generates the `.proto` files of the pull request and of `main`, runs `buf lint` on the former, and runs `buf breaking` with the `PACKAGE` rules to reject changes that break compatibility with `main`.

The conventions of the encoding (enum zero values, field presence, oneofs, absent and unknown values) are described in the [figure schema reference](../reference/figure-schema.md#protocol-buffers-encoding).

### JSON is a secondary encoding

JSON (`.fig.json`, or any `.json` file) is a supported secondary encoding, produced with `serde`, for debugging, for tools that read text, and for simple web pages. It describes the same model as the Protocol Buffers encoding, so a figure converts between the two without loss other than that noted in the [figure schema reference](../reference/figure-schema.md#json-encoding). A JSON Schema of the JSON encoding is generated from the Rust types with `schemars`, as one file per module, by `cargo run -p ironlab-ir --bin generate-schema` into `target/ironlab-schema/`. The JSON Schema is an optional artefact for tools that want one; it is not a compatibility check.

## Rationale

Protocol Buffers was chosen as the default because it is compact and fast for the large numeric arrays that figures hold, and because generated `.proto` files give every mainstream language typed access to a figure. For a figure with arrays of 10⁷ values each, the measurements were as follows.[^measurement]

| Encoding | Size | Encode | Decode |
| --- | --- | --- | --- |
| Protocol Buffers | 160 MB | 54 ms | 53 ms |
| JSON (pretty-printed) | 501 MB | 577 ms | 931 ms |

The Protocol Buffers file is a third of the size of the JSON file, and is encoded about ten times and decoded about seventeen times faster. Arrays are packed IEEE 754 doubles, so NaN, infinities and negative zero are stored natively rather than as `null`.

JSON remains supported because it can be read and edited by hand, diffed, and produced by tools and web pages that have no Protocol Buffers library.

## Alternatives considered

- **JSON as the only format.** This was the original decision while the MVP was developed, because JSON is readable and easy to debug. The measurements above show that it does not scale to the data sizes of scientific figures, and a JSON Schema does not by itself give other languages efficient typed access.
- **Hand-written `.proto` files compiled with `prost-build`.** Committing `.proto` files as the source of truth would require either generated Rust types that the rest of IronLAB would use in place of idiomatic domain types, or a second, hand-maintained definition of the model. Declaring the wire types once in Rust and rendering the `.proto` files from that declaration keeps one definition.
- **The `proto_rs` crate**, which derives `.proto` definitions and encodings from annotated Rust types, was evaluated and rejected.[^proto-rs]

## Consequences

- Every consumer of a figure (the viewer, the PDF exporter, the gallery, and in future any socket protocol or client library) works from one model, so adding a consumer does not change the others.
- A `.fig` file is a complete, reproducible description of a figure that can be archived with results and reopened in the viewer, and the same bytes can be sent between processes.
- The model is a compatibility surface. Changes to it require a version increment and, before implementation, a reviewed definition of the new data structures, as the project rules require. `buf breaking` enforces compatibility of the Protocol Buffers encoding mechanically.
- Every change to a domain type requires a matching change to its wire type and conversion. The round-trip tests of `ironlab-ir` fail when the two disagree.
- Because the schemas are not committed, readers of the documentation or other projects obtain them by running the generators.

[^measurement]: The same figure was encoded and decoded in each format. The JSON was pretty-printed, as `.fig.json` files are written.

[^proto-rs]: `proto_rs` was rejected for four reasons found during its evaluation. First, the `.proto` output that it generated was invalid, because it contained duplicate enum value names. Second, it had defects that affected the IronLAB model: fields named `value` were mishandled, messages that mixed explicit and implicit field numbers were mishandled, negative zero was lost on a round trip, and `usize` fields were not supported. Third, its source repository returned HTTP 404, so defects could not be reported or investigated. Fourth, its documentation builds were failing.
