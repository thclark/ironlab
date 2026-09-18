#!/usr/bin/env bash
# Build the documentation site: generate the gallery with IronLAB's own renderer, run zensical in strict mode, then
# stamp the site's own assets with a content hash.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run --release -p ironlab-gallery -- docs docs/gallery
uvx --from zensical==0.0.62 zensical build --clean --strict
# Zensical does not fingerprint the site's own stylesheets, images and PDFs, so a browser could pair a freshly
# deployed page with ones it cached from the last deploy (see scripts/docs-cachebust.py).
python3 scripts/docs-cachebust.py site
