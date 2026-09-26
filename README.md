# ironlab
An interactive plotting tool for scientific computing in Rust

WARNING: This repo is moving SUPER fast, and I do NOT care about breaking things (yet). If you use it, pin to an exact version.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org/). It covers [getting started](https://ironlab.org/guides/getting-started/), [using the viewer](https://ironlab.org/guides/viewer/), [embedding figures in a web page](https://ironlab.org/guides/embedding/), and the [architecture](https://ironlab.org/reference/architecture/).

The [gallery](https://ironlab.org/gallery/) shows every figure IronLAB can draw, live in the page and drawn by the same engine as the viewer, each with the code that produced it and the downloadable vector PDF it exports to.

## Development shortcuts

You need cargo and uv installed, and the browser toolchain that `scripts/build-web.sh --help` names, because the docs build compiles the bundle that shows the gallery figures live.
To build the gallery, the docs, then serve them locally:
```
cargo run --release -p ironlab-gallery -- docs docs/gallery
./scripts/build-docs.sh
uvx --from zensical==0.0.62 zensical serve
```

To open the gallery figures in the interactive viewer, either every figure or only the figures whose slugs are given:
```
cargo run -p ironlab-gallery -- view
cargo run -p ironlab-gallery -- view image mapped_image indexed_image correlation_peak
```
