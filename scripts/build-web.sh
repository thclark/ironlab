#!/usr/bin/env bash
# Build the browser bundle of IronLAB: compile `ironlab-web` for wasm32-unknown-unknown in the size-tuned `web` profile,
# generate its JavaScript bindings with wasm-bindgen, shrink the module with wasm-opt, and assemble the files a page
# needs into one directory. That directory is what a release attaches as `ironlab-web-<version>.tar.gz`, for
# publications that host their own figures, and what the documentation build serves at ironlab.org/embed/.
#
# Usage:
#
#     scripts/build-web.sh [--out DIR] [--profile NAME] [--no-opt]
#
# `--out` names the output directory (default `dist`). `--profile` names the Cargo profile (default `web`, defined in
# the root Cargo.toml). `--no-opt` skips wasm-opt, the slowest step, for local iteration on a module whose size does
# not matter yet; a bundle built without it is several times larger and must not be released.
#
# wasm-bindgen-cli must be the exact version of the wasm-bindgen crate in Cargo.lock, because the JavaScript it writes
# and the module it reads only agree within one version, so the version is read from the lockfile rather than pinned
# here. The stack size of the module is set by .cargo/config.toml, which Cargo applies to every build of the wasm32
# target, so this script passes nothing for it.
#
# The loader `www/ironlab.js` carries two placeholders that this script fills: `__IRONLAB_SHADOW_CSS__`, once, becomes
# the contents of `www/figure.css` as a JSON string, so that the stylesheet of the figure's shadow root ships inside
# the loader rather than as a file the page must also serve; and `__IRONLAB_BUILD__`, twice, becomes the first eight
# hex digits of the SHA-256 of the optimised module, which the loader appends to the URLs it fetches so that a browser
# never pairs a cached module with a loader from a different build.
set -euo pipefail
cd "$(dirname "$0")/.."

out=dist
profile=web
opt=1
while [ $# -gt 0 ]; do
  case "$1" in
    --out)
      [ $# -ge 2 ] || { echo "error: --out needs a directory" >&2; exit 2; }
      out=$2
      shift 2
      ;;
    --profile)
      [ $# -ge 2 ] || { echo "error: --profile needs a profile name" >&2; exit 2; }
      profile=$2
      shift 2
      ;;
    --no-opt)
      opt=0
      shift
      ;;
    -h|--help)
      sed -n '2,/^set -euo pipefail/{/^set -euo pipefail/d;s/^# \{0,1\}//p;}' "$0"
      exit 0
      ;;
    *)
      echo "error: unknown option $1 (see --help)" >&2
      exit 2
      ;;
  esac
done

# Every tool is checked before anything is built, so that a missing one is reported with the command that installs
# it rather than as a failure part way through.
missing() {
  echo "error: $1" >&2
  echo "       $2" >&2
  exit 2
}
command -v rustup >/dev/null 2>&1 \
  || missing "rustup is not installed; it is what installs the wasm32 target." "https://rustup.rs"
rustup target list --installed | grep -qx wasm32-unknown-unknown \
  || missing "the wasm32-unknown-unknown target is not installed." "rustup target add wasm32-unknown-unknown"
command -v jq >/dev/null 2>&1 \
  || missing "jq is not installed; it reads the wasm-bindgen version from Cargo.lock." \
             "brew install jq (macOS) or sudo apt-get install jq (Debian and Ubuntu)"
command -v python3 >/dev/null 2>&1 \
  || missing "python3 is not installed; it fills the placeholders of www/ironlab.js." \
             "brew install python (macOS) or sudo apt-get install python3 (Debian and Ubuntu)"
# `--locked` makes cargo refuse rather than rewrite a lockfile that no longer matches the manifests, so the version
# read here is always the one that will be linked. A dependency graph with two versions of wasm-bindgen would have no
# single answer, so it is an error.
wanted=$(cargo metadata --format-version 1 --locked \
  | jq -r '[.packages[] | select(.name == "wasm-bindgen") | .version] | unique
           | if length == 1 then .[0] else error("Cargo.lock must hold exactly one version of wasm-bindgen, found \(.)") end')
command -v wasm-bindgen >/dev/null 2>&1 \
  || missing "wasm-bindgen-cli is not installed." "cargo install wasm-bindgen-cli --version $wanted --locked"
installed=$(wasm-bindgen --version | awk '{print $2}')
[ "$installed" = "$wanted" ] \
  || missing "wasm-bindgen-cli is $installed but Cargo.lock has wasm-bindgen $wanted; the two must match exactly." \
             "cargo install wasm-bindgen-cli --version $wanted --locked --force"
if [ "$opt" = 1 ]; then
  command -v wasm-opt >/dev/null 2>&1 \
    || missing "wasm-opt is not installed." \
               "brew install binaryen (macOS) or sudo apt-get install binaryen (Debian and Ubuntu), or pass --no-opt"
fi

# Cargo names the output directory after the profile, except for the two built-in profiles that share `debug` and the
# one that shares `release`.
case "$profile" in
  dev|test) profile_dir=debug ;;
  bench) profile_dir=release ;;
  *) profile_dir=$profile ;;
esac
# Cargo writes under CARGO_TARGET_DIR when it is set, as a worktree sharing a build directory does.
target="${CARGO_TARGET_DIR:-target}"
module="$target/wasm32-unknown-unknown/$profile_dir/ironlab_web.wasm"

echo "==> Compiling ironlab-web for wasm32-unknown-unknown ($profile profile)"
cargo build --locked --profile "$profile" --target wasm32-unknown-unknown -p ironlab-web
[ -f "$module" ] || { echo "error: cargo produced no $module" >&2; exit 1; }

# The bindings are regenerated from scratch each time, so that a file wasm-bindgen no longer writes cannot survive
# from an earlier run and be copied into the bundle.
bindgen="$target/web-bindgen"
echo "==> Generating the JavaScript bindings with wasm-bindgen $installed"
rm -rf "$bindgen"
wasm-bindgen --target web --out-dir "$bindgen" --out-name ironlab_core "$module"

mkdir -p "$out"
if [ "$opt" = 1 ]; then
  echo "==> Optimising the module with $(wasm-opt --version)"
  # wasm-opt enables the features named in the module's own target_features section; the flags below repeat the
  # features rustc enables by default for wasm32-unknown-unknown so that the module still validates should a tool
  # upstream have dropped that section. Enabling a feature the module does not use changes nothing.
  wasm-opt -Oz --strip-debug --strip-producers \
    --enable-mutable-globals --enable-sign-ext --enable-reference-types --enable-multivalue \
    --enable-bulk-memory --enable-nontrapping-float-to-int \
    -o "$out/ironlab_core_bg.wasm" "$bindgen/ironlab_core_bg.wasm"
else
  echo "==> Skipping wasm-opt (--no-opt)"
  cp "$bindgen/ironlab_core_bg.wasm" "$out/ironlab_core_bg.wasm"
fi

echo "==> Assembling $out/"
www=crates/ironlab-web/www
cp "$bindgen/ironlab_core.js" "$out/ironlab_core.js"
cp "$bindgen/ironlab_core.d.ts" "$out/ironlab_core.d.ts"
# The module's own declaration file is written for every current version of wasm-bindgen, but it is not part of the
# loader's contract, so its absence is not an error.
[ -f "$bindgen/ironlab_core_bg.wasm.d.ts" ] && cp "$bindgen/ironlab_core_bg.wasm.d.ts" "$out/ironlab_core_bg.wasm.d.ts"
cp "$www/ironlab.css" "$out/ironlab.css"
cp "$www/README.md" "$out/README.md"
cp LICENSE "$out/LICENSE"

# macOS ships shasum and Linux ships sha256sum; either prints the digest first.
sha256_prefix() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -c1-8
  else
    shasum -a 256 "$1" | cut -c1-8
  fi
}
build=$(sha256_prefix "$out/ironlab_core_bg.wasm")

# The substitution is done in Python rather than sed because the stylesheet may contain any of sed's delimiters and
# escapes. Each placeholder is counted before it is replaced, so that a loader edited to use one more or one fewer
# fails here rather than in a browser. The build hash is substituted first, so that the stylesheet's text, whatever it
# holds, is never itself searched for a placeholder.
python3 - "$www/ironlab.js" "$www/figure.css" "$build" "$out/ironlab.js" <<'PY'
import json
import sys

source, stylesheet, build, target = sys.argv[1:]
with open(source, encoding="utf-8") as handle:
    loader = handle.read()
with open(stylesheet, encoding="utf-8") as handle:
    css = handle.read()

for placeholder, expected in (("__IRONLAB_SHADOW_CSS__", 1), ("__IRONLAB_BUILD__", 2)):
    found = loader.count(placeholder)
    if found != expected:
        sys.exit(f"error: {source} contains {placeholder} {found} time(s); expected {expected}")

loader = loader.replace("__IRONLAB_BUILD__", build)
# The JSON encoding of the stylesheet, without its surrounding quotes, because the loader already has them.
loader = loader.replace("__IRONLAB_SHADOW_CSS__", json.dumps(css)[1:-1])
with open(target, "w", encoding="utf-8") as handle:
    handle.write(loader)
PY

# The sizes that matter to a page are the compressed ones, since every host that serves the bundle compresses it;
# `gzip -9` is the ceiling of what gzip achieves, and brotli would do a little better still.
bytes_of() { wc -c < "$1" | tr -d ' '; }
gzipped_bytes_of() { gzip -9 -c "$1" | wc -c | tr -d ' '; }
files=(ironlab_core_bg.wasm ironlab_core.js ironlab.js ironlab.css)
total_raw=0
total_gz=0
rows=()
for name in "${files[@]}"; do
  raw=$(bytes_of "$out/$name")
  gz=$(gzipped_bytes_of "$out/$name")
  total_raw=$((total_raw + raw))
  total_gz=$((total_gz + gz))
  rows+=("$name $raw $gz")
done
rows+=("total $total_raw $total_gz")

echo
echo "Bundle $build in $out/"
printf '%-24s %12s %12s\n' "file" "bytes" "gzip -9"
for row in "${rows[@]}"; do
  # shellcheck disable=SC2086
  printf '%-24s %12s %12s\n' $row
done

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### ironlab-web bundle \`$build\`"
    echo
    echo "| File | Bytes | gzip -9 |"
    echo "| --- | ---: | ---: |"
    for row in "${rows[@]}"; do
      # shellcheck disable=SC2086
      printf '| %s | %s | %s |\n' $row
    done
  } >> "$GITHUB_STEP_SUMMARY"
fi
