#!/usr/bin/env python3
"""Reject a commit message that attributes the commit to an AI assistant or its vendor.

A contributor is a person who takes responsibility for the code, which an AI assistant cannot do, so commits carry no
`Co-Authored-By` trailer, "Generated with" footer or similar line naming one (see CLAUDE.md). Coding assistants add
such lines by default, so this check runs at the `commit-msg` stage and refuses the commit rather than relying on each
assistant being configured correctly.

Usage, as pre-commit runs it:

    python3 scripts/check-commit-message-attribution.py [MESSAGE_FILE]

Without MESSAGE_FILE the check reads the repository's `COMMIT_EDITMSG`, where git writes the message before it runs
the `commit-msg` hook. That is the usual case: the top-level `exclude` of .pre-commit-config.yaml removes every path
under `.git/`, the message file among them, so pre-commit passes the hook no filename.

Lines that git strips from the message are ignored: comment lines, and everything below the scissors line that a
verbose commit writes above the diff. The diff therefore never trips the check, even when it contains these patterns.
"""

import re
import subprocess
import sys

SCISSORS = "# ------------------------ >8 ------------------------"

ASSISTANTS = r"(claude|anthropic|copilot|chatgpt|openai|gemini|cursor)"

PATTERNS = [
    re.compile(pattern, re.IGNORECASE)
    for pattern in (
        rf"^co-authored-by:.*\b{ASSISTANTS}\b",
        r"noreply@anthropic\.com",
        rf"generated (with|by) \W*{ASSISTANTS}",
        r"claude\.com/claude-code",
        rf"\b(prepared|written|authored) (by|with) {ASSISTANTS}\b",
    )
]


def main(path):
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    text = text.split(SCISSORS, 1)[0]
    found = []
    for number, line in enumerate(text.splitlines(), start=1):
        if line.startswith("#"):
            continue
        if any(pattern.search(line) for pattern in PATTERNS):
            found.append(f"  line {number}: {line.strip()}")
    if found:
        print("The commit message attributes the commit to an AI assistant; remove these lines:", file=sys.stderr)
        print("\n".join(found), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    if len(sys.argv) > 2:
        sys.exit("usage: check-commit-message-attribution.py [MESSAGE_FILE]")
    if len(sys.argv) == 2:
        message = sys.argv[1]
    else:
        message = subprocess.run(
            ["git", "rev-parse", "--git-path", "COMMIT_EDITMSG"], capture_output=True, text=True, check=True
        ).stdout.strip()
    sys.exit(main(message))
