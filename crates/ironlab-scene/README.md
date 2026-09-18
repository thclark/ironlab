# ironlab-scene

Layout, scales and scene compilation from the figure model to a display list, for [IronLAB](https://ironlab.org), an interactive plotting tool for scientific computing in Rust.

The crate's `compile` function takes a figure and a text engine and returns a scene: a backend-neutral display list of paths, glyph runs, images and groups measured in points, a hit map that relates a pointer position back to the figure, and any warnings raised along the way. It is the only place in IronLAB where geometry is decided. Laying out tiles, axes, titles, labels and legends, generating ticks, choosing automatic limits, applying colormaps, extracting contours, scaling quivers, projecting three-dimensional axes and sorting their faces by depth all happen here, once.

Compilation never fails. An artist whose data cannot be drawn is skipped and text that cannot be typeset is drawn as its source, each becoming a warning in the scene rather than an error. Large series are thinned to what the current view can resolve before their geometry reaches the display list, so that every backend draws the same thinned picture.

## Where this crate sits

IronLAB is a Cargo workspace. `ironlab-scene` sits between the figure model and the backends: it depends on `ironlab-ir` and `ironlab-text`, and both `ironlab-pdf` and `ironlab-viewer` depend on it. Because the screen and the PDF page receive identical geometry, they cannot disagree about what a figure looks like.

Most users should depend on the [`ironlab`](https://crates.io/crates/ironlab) facade crate rather than on this one. Depend on `ironlab-scene` directly only when you need this layer on its own, for instance to write a rendering backend of your own against the display list.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org), and the [architecture reference](https://ironlab.org/reference/architecture/) describes the display list, the hit map and the thinning rules in full. The source is at [github.com/thclark/ironlab](https://github.com/thclark/ironlab).

IronLAB is moving quickly and breaking changes are expected. Pin to an exact version.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).

If this conflicts with your use case, please raise an issue on the IronLAB repository and describe what you're trying to do. For commercial projects we can simply arrange a one-off license fee and for non-profit / academic efforts we may waive the fee.
