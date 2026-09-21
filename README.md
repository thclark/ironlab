# ironlab
An interactive plotting tool for scientific computing in Rust

WARNING: This repo is moving SUPER fast, and I do NOT care about breaking things (yet). If you use it, pin to an exact version.

## Documentation

The documentation site is at [ironlab.org](https://ironlab.org/). It covers [getting started](https://ironlab.org/guides/getting-started/), [using the viewer](https://ironlab.org/guides/viewer/), and the [architecture](https://ironlab.org/reference/architecture/).

The [gallery](https://ironlab.org/gallery/) shows every figure IronLAB can draw, each with the code that produced it and the downloadable vector PDF it exports to.

## Development shortcuts

You need cargo and uv installed.
To build the gallery, the docs, then serve them locally:
```
cargo run --release -p ironlab-gallery -- docs docs/gallery
./scripts/build-docs.sh
uvx --from zensical==0.0.62 zensical serve
```
