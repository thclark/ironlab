#!/usr/bin/env python3
"""Stamp the site's own static assets with a content hash in the built HTML.

Zensical fingerprints the theme's bundled assets (``main.ce62732f.min.css``) but
emits ``extra_css``, ``extra_javascript`` and anything linked from an override
template verbatim. GitHub Pages serves every file with ``max-age=600``, so for up
to ten minutes after a deploy a browser can combine the new HTML with a stylesheet
it cached from the previous one.

The gallery makes this matter more than it would elsewhere: every release renders
each figure again under the same file name, so a cached ``surf.png`` or
``surf.pdf`` can outlive the figure it shows.

This rewrites every ``href``/``src`` in the built HTML that resolves to a
stylesheet, script, image, font or PDF inside the site to ``...?v=<hash>``, the
first eight hex digits of the file's SHA-256. A changed file gets a new URL; an
unchanged one keeps its URL and its cache entry.

Usage:

    scripts/build-docs.sh               # runs this as its last step
    scripts/docs-cachebust.py [SITE_DIR]

Links that already carry a query or fragment, and links that do not resolve to a
file, are left alone. The HTML pages themselves cannot be stamped, because their
URLs are the site's addresses; a page may therefore be up to ten minutes stale
after a deploy, but it is never paired with assets from a different build.
"""

from __future__ import annotations

import hashlib
import os
import re
import sys
from urllib.parse import unquote

HREF = re.compile(r'((?:href|src)=")([^"]+)(")')
SKIP_PREFIXES = ("http://", "https://", "//", "#", "mailto:", "data:", "javascript:")
ASSET_SUFFIXES = (".css", ".js", ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".woff", ".woff2", ".pdf")


def resolve(site_dir: str, page: str, link: str) -> str:
    """Return the filesystem path a link points at."""
    target = unquote(link)
    if target.startswith("/"):
        full = os.path.join(site_dir, target.lstrip("/"))
    else:
        full = os.path.join(os.path.dirname(page), target)
    return os.path.normpath(full)


def main(argv: list[str]) -> int:
    site_dir = argv[1] if len(argv) > 1 else "site"
    if not os.path.isdir(site_dir):
        print(f"error: {site_dir}/ does not exist — run scripts/build-docs.sh first", file=sys.stderr)
        return 2

    pages = [
        os.path.join(root, name)
        for root, _, names in os.walk(site_dir)
        for name in names
        if name.endswith(".html")
    ]

    digests: dict[str, str] = {}
    stamped = 0

    def stamp(page: str, match: re.Match[str]) -> str:
        nonlocal stamped
        link = match.group(2)
        if link.startswith(SKIP_PREFIXES) or "?" in link or "#" in link:
            return match.group(0)
        if not link.lower().endswith(ASSET_SUFFIXES):
            return match.group(0)
        path = resolve(site_dir, page, link)
        if not os.path.isfile(path):
            return match.group(0)
        if path not in digests:
            with open(path, "rb") as handle:
                digests[path] = hashlib.sha256(handle.read()).hexdigest()[:8]
        stamped += 1
        return f"{match.group(1)}{link}?v={digests[path]}{match.group(3)}"

    for page in sorted(pages):
        with open(page, encoding="utf-8") as handle:
            html = handle.read()
        rewritten = HREF.sub(lambda match: stamp(page, match), html)
        if rewritten != html:
            with open(page, "w", encoding="utf-8") as handle:
                handle.write(rewritten)

    print(f"{stamped} references to {len(digests)} assets stamped across {len(pages)} pages")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
