# IronLAB

IronLAB is a Rust library and application for building scientific figures, exploring them interactively and exporting them for publication. It offers three things.

- **A retained figure model.** A figure is a tree of nodes (the figure, its axes and their plots) held in memory and saved as a compact Protocol Buffers `.fig` file, or as JSON. Every node has a stable identifier, and every property that affects the drawing is stored in the model, so a saved figure reopens exactly as it was built. Other languages can read and write figures through `.proto` files generated from the model.
- **An interactive viewer.** A desktop window shows one or more figures as tabs, in which axes can be panned, zoomed and rotated, plots can be hidden or shown from the legend, and the properties of any object can be changed in a [property editor](guides/viewer.md#the-property-editor). Every interaction sets a property of the figure model, and can be undone or discarded, so the view on screen is always a figure that can be saved or exported.
- **Publication-quality PDF export.** A figure is exported as a single-page PDF whose page is exactly the size of the figure, with fonts embedded and text that can be selected and searched. Labels may contain LaTeX mathematics, which IronLAB typesets itself: no TeX installation is needed to build, view or export a figure.

The API is modelled on MATLAB. Plotting functions carry MATLAB's names and argument order (`plot`, `scatter`, `contour`, `quiver`, `surf`, `image` and their relatives; MATLAB's `imagesc` is `mapped_image`), and their defaults follow MATLAB where MATLAB has an equivalent.

## A minimal example

The following program plots two curves with LaTeX legend entries and axis labels, exports the figure to PDF and opens it in the viewer.

```rust
use ironlab::prelude::*;

fn main() -> Result<(), ironlab::Error> {
    let x = linspace(0.0, 2.0 * std::f64::consts::PI, 200);
    let sin: Vec<f64> = x.iter().map(|x| x.sin()).collect();
    let cos: Vec<f64> = x.iter().map(|x| x.cos()).collect();

    let mut fig = Figure::new().size_mm(120.0, 80.0).title("Trigonometric functions");
    let mut ax = fig.axes(0, 0);
    ax.plot(&x, &sin).display_name("$\\sin x$");
    ax.plot(&x, &cos).display_name("$\\cos x$").dash(Dash::Dashed);
    ax.xlabel("$x$").ylabel("$f(x)$").legend(LegendLocation::NorthEast);

    fig.export_pdf("trigonometric.pdf")?;
    fig.show()
}
```

The [getting started guide](guides/getting-started.md) explains each step and every supported plot type.

## Map of the documentation

- **Guides** explain how to use IronLAB.
    - [Getting started](guides/getting-started.md) covers building figures with the Rust API, saving and loading them, exporting PDF for LaTeX documents and opening the viewer.
    - [Using the viewer](guides/viewer.md) describes the viewer's tools, gestures and keyboard shortcuts, and how to edit a figure live in its property editor.
- **[Gallery](gallery/index.md)** shows every example figure with the exact source code that produced it.
- **Reference** describes the system precisely.
    - [Figure schema](reference/figure-schema.md) explains every entity of the figure model, its Protocol Buffers and JSON encodings, and the rules for versioning it.
    - [Architecture](reference/architecture.md) describes the crates, how a figure becomes pixels or a PDF page, and how text is resolved.
- **[Architecture decisions](adrs/index.md)** records the significant design decisions and the reasons for them.
- **[Background](background/index.md)** contains the discussion from which the design emerged.
- **Conventions** describe how the project is developed: [branching](conventions/git-branching.md), [commits and versioning](conventions/git-commits-and-versioning.md) and [pull requests](conventions/git-pull-requests.md). The [contributing guide](https://github.com/thclark/ironlab/blob/main/CONTRIBUTING.md) sets out how contributions are licensed.
