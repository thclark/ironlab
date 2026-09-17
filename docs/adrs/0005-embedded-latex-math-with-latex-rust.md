# ADR 0005: Embedded LaTeX mathematics with latex-rust

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0004](0004-pdf-first-export-with-krilla.md), [ADR 0007](0007-mvp-scope.md)

## Context

Scientific figures need typeset mathematics in titles, axis labels and legend entries, and IronLAB's exported figures must look right in LaTeX documents. The [background discussion](../background/concept-discussion.md) worked through the options in several steps.

- **Where text is typeset.** Three strategies were considered: a self-contained figure whose text is typeset by IronLAB; a PGF/TikZ backend that lets the document's LaTeX draw everything; and a split output of graphics plus a LaTeX overlay for text. Only the self-contained strategy makes a figure's appearance a function of the figure alone, which is what makes saved figures reproducible and regression tests meaningful. Scaling a figure layout to fonts chosen by each host document was judged not worth its complexity; additional font sets can be built in instead.
- **How mathematics is typeset.** Running an installed TeX engine and reading glyph positions from its output supports every macro, but requires a TeX installation on every machine, costs a process launch per string, and makes output depend on the installed TeX version. Embedding a typesetter removes all three problems. Among Rust typesetters, latex-rust lays out LaTeX mathematics with a TeX-faithful box model against OpenType MATH fonts, returns a box tree with width, height and depth rather than only a rendered image, and returns an error for unsupported input rather than a wrong rendering.
- **Offering both.** Keeping a TeX shell-out as a second implementation for exotic macros was rejected as a niche requirement that would double the text path.
- **What the figure stores.** Storing resolved glyph runs in the figure was considered and rejected. With the typesetter compiled into IronLAB, the renderer can always resolve source text; glyph identifiers are meaningful only with the exact font file, so stored runs would not freeze appearance without also embedding fonts; and storing source keeps text editable. A frozen, self-contained form of a figure already exists: the exported PDF, which embeds the font subsets.

## Decision

IronLAB typesets LaTeX mathematics itself with latex-rust, and never runs an external TeX program.

- **Source in the IR.** The figure model stores text as source with an interpreter (`latex` or `none`), as described in the [figure schema reference](../reference/figure-schema.md#text). With the `latex` interpreter, segments delimited by `$…$` are mathematics and the rest is plain text.
- **Lazy resolution with a memo.** Text is resolved by the text engine only when the scene compiler lays out a figure. Results are memoised in memory on the source, the interpreter and the size, so each distinct label is typeset once per process; no disk cache is kept.
- **One path.** Mathematics is converted from latex-rust's box tree into positioned glyphs and rules, which enter the shared display list like any other text. latex-rust's own renderers, including its egui renderer, are not used, so the screen and the PDF draw identical glyphs (see [ADR 0003](0003-shared-scene-compiler-and-display-list.md)).
- **Fallback and warning.** Mathematics that cannot be parsed or laid out is drawn as its raw source in the text font, and a warning naming the node is recorded and shown in the viewer's problems indicator. A label never prevents a figure from being built, drawn or exported.
- **Fonts.** Only the STIX Two font set is provided in the MVP: STIX Two Text for plain text (shaped with HarfRust) and STIX Two Math for mathematics. The math font bytes are exactly those that latex-rust embeds, so glyph identifiers from layout always match the font that both backends draw with. The fonts are distributed under the SIL Open Font License, and the figure's provenance records the typesetter version and font names.
- **TeX conventions that latex-rust 1.0.2 does not apply.** Before layout, a hyphen in mathematics is replaced by the minus sign U+2212, and unstyled Latin letters and lowercase Greek letters are replaced by their Unicode mathematical italic forms, while digits and the contents of `\mathrm`, `\text` and `\operatorname` stay upright. Plain text is never altered.
- **Script sizes.** latex-rust's glyph records carry no size, and its own renderers draw scripts at full size. IronLAB derives each glyph's scale from its box and snaps it to the font's script and script-script scale factors.
- **Stack safety.** latex-rust parses and lays out recursively. Each mathematics segment is therefore typeset on a helper thread with a large stack, and source nested more deeply than a fixed limit is rejected before parsing and handled by the fallback, so no input can overflow a stack.

## Consequences

- Building, viewing and exporting figures with real mathematics needs no TeX installation, no shell escape and no administrator rights, on any machine that runs the IronLAB binary.
- Only the subset of LaTeX mathematics that latex-rust supports is available, and user-defined macros are not. Unsupported input is visible in the figure and in the problems indicator rather than silently wrong.
- IronLAB depends on a young crate with a single principal author. The version is pinned exactly, and the crate's licence permits vendoring or forking it. Because figures store source, a change of typesetter affects how figures are drawn in future, not the files themselves.
- Figure fonts do not follow the host document's fonts. Journals that require Times-like fonts are served by STIX Two; other font sets, such as Latin Modern, can be added later as further font sets.
- Workarounds for latex-rust's script sizing and TeX conventions live in IronLAB and must be revisited when latex-rust changes.

Follow-up work is tracked in GitHub issues:

<!-- issues -->
