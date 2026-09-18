# ironlab

A MATLAB-flavoured Rust API for building, viewing and exporting scientific figures.

A figure is built by placing axes in a grid of tiles and adding plots to them with functions named after their MATLAB equivalents: `plot`, `scatter`, `contour`, `quiver`, `surf` and their relatives. Each plotting function returns a handle whose chained setters change the properties of the new plot, in the way that MATLAB's name–value arguments do. The figure can then be shown in the interactive viewer, saved as a `.fig` file or as JSON, or exported to a single-page PDF whose page is exactly the size of the figure, with fonts embedded and text that can be selected and searched. Labels may contain LaTeX mathematics, which IronLAB typesets itself, so no TeX installation is needed.

## Where this crate sits

IronLAB is a Cargo workspace, and this crate is its facade. It writes directly to the retained figure model of `ironlab-ir`, and depends on `ironlab-text` for typesetting, `ironlab-scene` for layout and geometry, `ironlab-pdf` for export and `ironlab-viewer` for the interactive window. This is the crate to depend on: the others are useful directly only when a particular layer is needed on its own.

## Usage

```shell
cargo add ironlab
```

```rust
use ironlab::prelude::*;

let x = linspace(0.0, 2.0 * std::f64::consts::PI, 200);
let sin: Vec<f64> = x.iter().map(|x| x.sin()).collect();
let cos: Vec<f64> = x.iter().map(|x| x.cos()).collect();

let mut fig = Figure::new().size_mm(120.0, 80.0).title("Trigonometric functions");
let mut ax = fig.axes(0, 0);
ax.plot(&x, &sin).display_name("$\\sin x$");
ax.plot(&x, &cos).display_name("$\\cos x$").dash(Dash::Dashed);
ax.xlabel("$x$").ylabel("$f(x)$").legend(LegendLocation::NorthEast);

assert!(fig.validate().is_valid());
```

Builder calls never panic because of inconsistent input, such as arrays of different lengths or a tile outside the layout. Such problems are reported by `Figure::validate`, and are checked again before a figure is exported or shown.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org). It covers [getting started](https://ironlab.org/guides/getting-started/), [using the viewer](https://ironlab.org/guides/viewer/) and the [architecture](https://ironlab.org/reference/architecture/), and the [gallery](https://ironlab.org/gallery/) shows every figure IronLAB can draw, each with the code that produced it and the PDF it exports to. The source is at [github.com/thclark/ironlab](https://github.com/thclark/ironlab).

IronLAB is moving quickly and breaking changes are expected. Pin to an exact version.

## Licence

This crate is licensed under the GNU Affero General Public License, either version 3 or (at your option) any later version (`AGPL-3.0-or-later`).
