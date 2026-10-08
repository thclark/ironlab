# ADR 0017: Embedded LaTeX mathematics with latex-rust applying TeX's conventions

**Status:** Accepted

**Supersedes:** [ADR 0005](0005-embedded-latex-math-with-latex-rust.md)

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0007](0007-mvp-scope.md), [ADR 0010](0010-pdf-export-with-a-raster-fallback.md)

## Context

[ADR 0005](0005-embedded-latex-math-with-latex-rust.md) chose to typeset LaTeX mathematics in process with latex-rust, from source stored in the figure, with a fallback and a warning for unsupported input. latex-rust 1.0.2, the version then available, did not apply several of TeX's conventions and could exhaust the stack on deeply nested input, so ADR 0005 also recorded the workarounds that IronLAB applied around it: rewriting hyphens and letters before layout, deriving the size of each glyph from its box, and rejecting deeply nested source before parsing.

Those defects were reported upstream in [#10](https://github.com/thclark/ironlab/issues/10), and latex-rust 2.0.0 fixed all of them. latex-rust now sets unstyled letters in mathematical italic, typesets a hyphen in mathematics as the minus sign U+2212, applies font switches such as `\mathrm` to every letter inside them, records the scale of each glyph, places accents by TeX's rule, and returns an error for input nested beyond a limit instead of overflowing the stack. latex-rust 2.1.0 also draws scripts with the font's OpenType script-style (`ssty`) glyphs and converts absolute TeX lengths using the size at which the mathematics is set. The reasons for embedding a typesetter, and for storing source rather than glyphs, are unchanged and are not repeated here.

## Decision

IronLAB typesets LaTeX mathematics itself with latex-rust, never runs an external TeX program, and relies on latex-rust to apply TeX's conventions.

- **Source in the IR.** The figure model stores text as source with an interpreter (`latex` or `none`), as described in the [figure schema reference](../reference/figure-schema.md#text). With the `latex` interpreter, segments delimited by `$…$` are mathematics and the rest is plain text.
- **Lazy resolution with a memo.** Text is resolved by the text engine only when the scene compiler lays out a figure. Results are memoised in memory on the source, the interpreter and the size, so each distinct label is typeset once per process; no disk cache is kept.
- **One path.** Mathematics is converted from latex-rust's box tree into positioned glyphs and rules, which enter the shared display list like any other text. latex-rust's own renderers are not used, so the screen and the PDF draw identical glyphs (see [ADR 0003](0003-shared-scene-compiler-and-display-list.md)).
- **No rewriting of source or output.** Mathematics is passed to latex-rust as the author wrote it, and each glyph is drawn with the identifier and at the scale that latex-rust gives it. Layout is told the size at which the label is set, so that absolute TeX lengths are correct at every size.
- **Fallback and warning.** Mathematics that cannot be parsed or laid out, including mathematics nested beyond latex-rust's limit, is drawn as its raw source in the text font, and a warning naming the node is recorded and shown in the viewer's problems indicator. A label never prevents a figure from being built, drawn or exported.
- **Stack safety.** latex-rust parses and lays out recursively and bounds the depth of that recursion. Its limit is safe on a stack of 2 MiB in an unoptimised build but not on a smaller one, and the text engine is called from threads whose remaining stack is unknown. Natively, each mathematics segment is therefore typeset on a helper thread with a stack of known size; in the browser, which has one thread, the linker gives that thread a large stack instead.
- **Fonts.** Only the STIX Two font set is provided: STIX Two Text for plain text (shaped with HarfRust) and STIX Two Math for mathematics. The math font bytes, and the parsed face used for metrics and outlines, are exactly those that latex-rust embeds and lays out with, so glyph identifiers from layout always match the font that every backend draws with. The fonts are distributed under the SIL Open Font License.
- **Provenance.** A figure's provenance records the typesetter's name and its version in separate fields, so that a change of version is not mistaken for a change of typesetter. The version recorded by default is checked by a test against the exact version to which the workspace pins latex-rust.

## Consequences

- Building, viewing and exporting figures with real mathematics needs no TeX installation, no shell escape and no administrator rights, on any machine that runs the IronLAB binary.
- Only the subset of LaTeX mathematics that latex-rust supports is available, and user-defined macros are not. Unsupported input is visible in the figure and in the problems indicator rather than silently wrong.
- The typeset appearance of mathematics is entirely latex-rust's. A defect in it is fixed upstream rather than worked around in IronLAB, and an upgrade of latex-rust may change how existing figures are drawn, which the provenance records.
- IronLAB depends on a young crate with a single principal author. The version is pinned exactly, and the crate's licence permits vendoring or forking it. Because figures store source, a change of typesetter affects how figures are drawn in future, not the files themselves.
- Mathematics nested more deeply than latex-rust's limit (about fifteen nested fractions) falls back to plain text. Labels rarely nest beyond four or five levels.
- Figure fonts do not follow the host document's fonts. Journals that require Times-like fonts are served by STIX Two; other font sets, such as Latin Modern, can be added later as further font sets.

Related work is tracked in GitHub issues:

- [#10: Report latex-rust typesetting defects upstream](https://github.com/thclark/ironlab/issues/10)
- [#63: Shrink the browser bundle](https://github.com/thclark/ironlab/issues/63), whose first step, embedding the math font once, latex-rust 2.0.0 completed
