#!/usr/bin/env bash
# Build the documentation site: generate the gallery with IronLAB's own renderer, build the browser bundle that shows
# the gallery's figures live, run zensical in strict mode, then stamp the site's own assets with a content hash.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run --release -p ironlab-gallery -- docs docs/gallery

# The bundle is built into a staging directory and only the four files a page loads are copied under docs/, where
# zensical publishes them at /embed/. The README and the LICENSE the bundle also carries stay behind: a Markdown file
# under docs/ becomes a page of the site, and the licence is the repository's own.
scripts/build-web.sh --out target/web-dist
mkdir -p docs/embed
for file in ironlab.js ironlab_core.js ironlab_core_bg.wasm ironlab.css; do
  cp "target/web-dist/$file" "docs/embed/$file"
done

uvx --from zensical==0.0.62 zensical build --clean --strict
# Zensical does not fingerprint the site's own stylesheets, images and PDFs, so a browser could pair a freshly
# deployed page with ones it cached from the last deploy (see scripts/docs-cachebust.py).
python3 scripts/docs-cachebust.py site
