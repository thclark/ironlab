#!/usr/bin/env bash
# Regenerate the committed schemas in schema/ from the Rust types, and fail if the result differs from what is staged.
#
# The pre-commit hook `generate-schemas` and the `schema` job of CI both run this script. Under pre-commit the
# unstaged changes are stashed first, so the working tree holds exactly what is about to be committed; in CI the index
# holds the commit that was checked out. In both cases a difference between the working tree and the index, or a file
# that git does not yet track, means that the committed schemas are stale. The regenerated files are left in place, so
# that the developer only has to stage them, as with a formatter.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo run --quiet -p ironlab-ir --bin generate-proto >/dev/null
cargo run --quiet -p ironlab-ir --bin generate-schema >/dev/null

untracked=$(git ls-files --others --exclude-standard -- schema/)
if git diff --quiet -- schema/ && [ -z "$untracked" ]; then
  exit 0
fi

echo "The committed schemas in schema/ are stale: they differ from those generated from the Rust types." >&2
echo "The regenerated files are in the working tree; review and stage them with 'git add schema/'." >&2
echo >&2
git diff --stat -- schema/ >&2
if [ -n "$untracked" ]; then
  echo "New files:" >&2
  echo "$untracked" | sed 's/^/  /' >&2
fi
exit 1
