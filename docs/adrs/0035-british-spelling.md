# ADR 0035: British spelling

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0034](0034-properties-are-grouped-by-the-part-they-draw.md)

## Context

The documentation was written in British English, while most names in the code used American spelling, such as `color`, `center`, `gray` and `normalize`. Because the JSON keys, the Protocol Buffers names, the property paths and the facade are generated from or mirror those names, the mixture reached every user of the format.

## Decision

Every name the project controls is spelt in British English: types, fields, variants, functions, files, JSON keys, Protocol Buffers names, property paths, facade methods, gallery names and documentation. For example, `color` becomes `colour`, `colormap` becomes `colourmap`, `center` becomes `centre`, `gray` becomes `grey` and `normalize` becomes `normalise`.

Names that belong to other software are quoted as that software spells them. These include the types and functions of dependencies, such as egui's `Color32`, CSS properties, and the functions of MATLAB, matplotlib and Plotly in comparisons, such as `colorbar`. Released ADRs are not edited.

## Alternatives considered

### American spelling throughout

American spelling would match CSS, most dependencies and the plotting libraries that IronLAB is compared with, so a user moving from matplotlib would type the same words. However, the project is written in British English, and one consistent language in the model, the code and the documentation matters more than matching the spelling of other software. An agent or program writing a figure reads the schema, which states every name.
