# ironlab-text

Font sets, text shaping and LaTeX mathematics typesetting for [IronLAB](https://ironlab.org), an interactive plotting tool for scientific computing in Rust.

The crate's `TextEngine` turns the source text of a label into positioned glyphs and filled rules measured in points. Plain text is shaped with HarfRust against the bundled STIX Two Text faces. When mathematics is requested, segments delimited by unescaped `$…$` are parsed and laid out with `latex-rust` against STIX Two Math, so no TeX installation is needed to build, view or export a figure. Glyph identifiers therefore always refer to the same font bytes that the renderers draw with. Resolved layouts and glyph outlines are memoised, and one engine is intended to be created once and shared, being both `Send` and `Sync`.

All output shares one coordinate convention: the origin is the left end of the baseline, x increases to the right and y increases downwards, so glyphs raised above the baseline have negative y.

## Where this crate sits

IronLAB is a Cargo workspace, and `ironlab-text` is one of its two foundation crates. It depends on no other IronLAB crate. The scene compiler, `ironlab-scene`, resolves every piece of a figure's text through it during layout, and the PDF and viewer backends draw the glyphs it produces and embed the fonts it holds.

Most users should depend on the [`ironlab`](https://crates.io/crates/ironlab) facade crate rather than on this one. Depend on `ironlab-text` directly only when you need this layer on its own, for instance to shape text or typeset mathematics into outlines outside an IronLAB figure.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org), and the [architecture reference](https://ironlab.org/reference/architecture/) explains how text is resolved and how the crates fit together. The source is at [github.com/thclark/ironlab](https://github.com/thclark/ironlab).

IronLAB is moving quickly and breaking changes are expected. Pin to an exact version.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).

The crate additionally embeds the STIX Two Text fonts, which are licensed separately under the SIL Open Font License; their terms are in `fonts/OFL.txt`.
