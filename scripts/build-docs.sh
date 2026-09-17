#!/usr/bin/env bash
# Build the documentation site: generate the gallery with IronLAB's own renderer, then run zensical in strict mode.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run --release -p ironlab-gallery -- docs docs/gallery
uvx --from zensical==0.0.62 zensical build --clean --strict
