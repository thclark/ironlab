# IronLAB

An interactive plotting tool for scientific computing, built in Rust.

IronLAB takes the best of MATLAB figures, plotters, matplotlib and plotly.js... building interactive figures with exact WYSIWYG publication-quality exports.

Read more at [ironlab.org](https://ironlab.org), or check out the [gallery](https://ironlab.org/gallery/) for code samples.

IronLAB is moving quickly and breaking changes are expected. Pin to an exact version.

## Where this crate sits

IronLAB is a Cargo workspace, and this crate is its facade. It writes directly to the retained figure model of `ironlab-ir`, and depends on `ironlab-text` for typesetting, `ironlab-scene` for layout and geometry, `ironlab-pdf` for export and `ironlab-viewer` for the interactive window. This is the crate to depend on: the others are useful directly only when a particular layer is needed on its own.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).

If this conflicts with your use case, please raise an issue on the IronLAB repository and describe what you're trying to do. For commercial projects we can simply arrange a one-off license fee and for non-profit / academic efforts we may waive the fee.
