#!/usr/bin/env python3
"""Decide the schema version that the rule-based tools require, and check SCHEMA_VERSION against it.

The schema version is never chosen by a person. Two tools decide it:

- strands.octue.com compares each single-file JSON Schema (``ironlab-figure`` and ``ironlab-edit``) with the latest
  version published to its strand and suggests the next version. The strands requirement is the highest suggestion
  over the strands; for a strand whose schema is unchanged (``equal``), the requirement is its latest version.
- ``buf breaking`` compares the generated Protocol Buffers files with those of ``main``. When it reports a breaking
  change, the buf requirement is the breaking bump of the latest published version: the next minor version while the
  major version is 0, and the next major version otherwise.

The required version is the higher of the two requirements, and the check passes when SCHEMA_VERSION equals it. The
single exception is the deliberate step to stable: SCHEMA_VERSION set to exactly 1.0.0 while the latest published
version is 0.x passes.

Usage (from CI; see .github/workflows/ci.yml):

    scripts/schema_version.py --current-file target/ironlab-schema/VERSION \\
        --strand ironlab-figure LATEST SUGGESTED CHANGE \\
        --strand ironlab-edit LATEST SUGGESTED CHANGE \\
        --buf breaking|compatible|skipped [--enforce] [--comment PATH] [--github-output PATH]

LATEST, SUGGESTED and CHANGE are the ``latest_version``, ``version`` and ``change`` outputs of
octue/publish-strand-version in suggest-only mode; LATEST is empty for a strand with no published version.

Without ``--enforce`` a mismatch is reported as a warning and the exit status is 0. The tests are in
scripts/test_schema_version.py and run with ``python3 -m unittest discover -s scripts``.
"""

import argparse
import re
import sys
from dataclasses import dataclass, field

STRANDS_URL = "https://strands.octue.com"
DEFAULT_ACCOUNT = "octue"
VERSION_LINE = 'pub const SCHEMA_VERSION: &str = "{}";'
VERSION_FILE = "crates/ironlab-ir/src/figure.rs"
CHANGES = ("equal", "initial", "patch", "minor", "major")
BUF_RESULTS = ("breaking", "compatible", "skipped")
STABLE = "1.0.0"

_SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?$")


class InputError(ValueError):
    """An input that the decision cannot be made from, such as a malformed version."""


def parse(version):
    """Return a sort key for a semantic version, in which a pre-release sorts below its release.

    >>> parse("0.10.0") < parse("0.11.0") < parse("1.0.0-rc.1") < parse("1.0.0")
    True
    """
    match = _SEMVER.match(version or "")
    if not match:
        raise InputError(f"{version!r} is not a semantic version of the form major.minor.patch")
    major, minor, patch, pre = match.groups()
    return (int(major), int(minor), int(patch), pre is None, pre or "")


def breaking_bump(version):
    """Return the version after a breaking change of ``version``: the next minor while the major is 0.

    >>> breaking_bump("0.10.0"), breaking_bump("0.10.3"), breaking_bump("1.2.3")
    ('0.11.0', '0.11.0', '2.0.0')
    """
    major, minor, _, _, _ = parse(version)
    return f"0.{minor + 1}.0" if major == 0 else f"{major + 1}.0.0"


def highest(versions):
    """Return the highest of the given versions, or None when there are none."""
    versions = [v for v in versions if v]
    return max(versions, key=parse) if versions else None


@dataclass
class Strand:
    """The suggestion of strands.octue.com for one strand."""

    name: str
    latest: str
    suggested: str
    change: str

    def __post_init__(self):
        if self.change not in CHANGES:
            raise InputError(f"strand {self.name}: change {self.change!r} is not one of {', '.join(CHANGES)}")
        if self.change != "initial" and not self.latest:
            raise InputError(f"strand {self.name}: a {self.change} change needs the latest published version")
        if self.latest:
            parse(self.latest)
        if self.change != "equal":
            parse(self.suggested)

    @property
    def requirement(self):
        """The version this strand requires: its latest version when unchanged, and its suggestion otherwise."""
        return self.latest if self.change == "equal" else self.suggested

    @property
    def reason(self):
        if self.change == "equal":
            return f"the schema equals published {self.latest}"
        if self.change == "initial":
            return "the strand has no published version, so its initial suggestion stands"
        return f"the schema has a {self.change} change from published {self.latest}"


@dataclass
class Decision:
    """The required version, how it was reached, and whether SCHEMA_VERSION meets it."""

    current: str
    required: str
    latest: str | None
    breaking: bool
    passed: bool
    stable_step: bool
    reasons: list = field(default_factory=list)
    sources: list = field(default_factory=list)


def decide(current, strands, buf):
    """Decide the required version from the strands' suggestions and the result of ``buf breaking``.

    ``current`` is SCHEMA_VERSION, ``strands`` a list of Strand, and ``buf`` one of "breaking", "compatible" or
    "skipped" (main has no generator to compare against).
    """
    parse(current)
    if not strands:
        raise InputError("at least one strand is needed")
    if buf not in BUF_RESULTS:
        raise InputError(f"buf result {buf!r} is not one of {', '.join(BUF_RESULTS)}")

    latest = highest(s.latest for s in strands)
    candidates = []  # (version, source, reason)
    for strand in strands:
        candidates.append((strand.requirement, f"strands ({strand.name})", strand.reason))

    buf_breaking = buf == "breaking"
    if buf_breaking:
        if latest:
            candidates.append(
                (breaking_bump(latest), "buf breaking", f"the .proto files break compatibility with main, so published {latest} takes a breaking bump")
            )
        else:
            # With nothing published there is no version to bump, so the strands' initial suggestion stands.
            candidates.append((None, "buf breaking", "the .proto files break compatibility with main, but nothing is published to bump"))

    required = highest(version for version, _, _ in candidates)
    sources = [source for version, source, _ in candidates if version == required]
    reasons = [(source, version, reason) for version, source, reason in candidates]

    breaking = buf_breaking or any(s.change == "major" for s in strands)
    stable_step = current == STABLE and latest is not None and parse(latest)[0] == 0
    passed = current == required or stable_step
    return Decision(current, required, latest, breaking, passed, stable_step and current != required, reasons, sources)


def report(decision):
    """Return the text printed to the log: concise on success, and complete on failure."""
    if decision.passed:
        if decision.stable_step:
            return (
                f"SCHEMA_VERSION {decision.current} is the deliberate step to stable from published "
                f"{decision.latest}; the rules alone would require {decision.required}."
            )
        return f"SCHEMA_VERSION {decision.current} is the version the rules require ({', '.join(decision.sources)})."
    lines = [
        f"SCHEMA_VERSION is {decision.current}, but the rule-based tools require {decision.required}, "
        f"which {' and '.join(decision.sources)} required.",
        "",
        "Each tool requires:",
    ]
    for source, version, reason in decision.reasons:
        lines.append(f"  - {source}: {version or 'nothing'}, because {reason}")
    lines += [
        "",
        f"Set this line in {VERSION_FILE}:",
        "",
        f"    {VERSION_LINE.format(decision.required)}",
    ]
    return "\n".join(lines)


def comment(decision, strands, buf, account=DEFAULT_ACCOUNT, enforced=True):
    """Return the Markdown body of the sticky pull request comment."""
    lines = ["## Schema version", ""]
    lines += ["| Strand | Published | Suggested | Change |", "|---|---|---|---|"]
    for s in strands:
        link = f"[`{s.name}`]({STRANDS_URL}/{account}/{s.name})"
        suggested = f"`{s.requirement}`" if s.requirement else "—"
        published = f"`{s.latest}`" if s.latest else "—"
        change = f"**{s.change}**" if s.change == "major" else s.change
        lines.append(f"| {link} | {published} | {suggested} | {change} |")
    buf_text = {
        "breaking": "**breaking**: the generated `.proto` files are incompatible with those of `main`",
        "compatible": "compatible with `main`",
        "skipped": "skipped, because `main` has no generator to compare against",
    }[buf]
    lines += ["", f"`buf breaking`: {buf_text}.", ""]
    lines.append(f"Required version: `{decision.required}`; `SCHEMA_VERSION` is `{decision.current}`.")
    lines.append("")
    if decision.passed and decision.stable_step:
        lines.append("Passed as the deliberate step to stable (1.0.0).")
    elif decision.passed:
        lines.append("Passed.")
    else:
        verb = "Failed" if enforced else "Not yet matched (enforced only on pull requests into `main`)"
        lines += [f"{verb}: set `{VERSION_LINE.format(decision.required)}` in `{VERSION_FILE}`."]
    if decision.breaking:
        lines += ["", "> [!WARNING]", "> The required change is breaking, so a commit must declare it with a "
                  "`BREAKING-CHANGE:` paragraph."]
    lines += ["", "<sub>Updated by the `schema` job of `.github/workflows/ci.yml` on every push.</sub>"]
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    current = parser.add_mutually_exclusive_group(required=True)
    current.add_argument("--current", help="SCHEMA_VERSION of the pull request")
    current.add_argument("--current-file", help="a file holding SCHEMA_VERSION, as generate-schema writes it")
    parser.add_argument(
        "--strand", nargs=4, action="append", required=True, metavar=("NAME", "LATEST", "SUGGESTED", "CHANGE")
    )
    parser.add_argument("--buf", required=True, choices=BUF_RESULTS)
    parser.add_argument("--account", default=DEFAULT_ACCOUNT)
    parser.add_argument("--enforce", action="store_true", help="exit with status 1 when the check fails")
    parser.add_argument("--comment", help="write the Markdown of the pull request comment to this path")
    parser.add_argument("--github-output", help="append required, breaking and passed to this GitHub output file")
    args = parser.parse_args(argv)

    try:
        if args.current_file:
            with open(args.current_file) as f:
                version = f.read().strip()
        else:
            version = args.current.strip()
        strands = [Strand(*values) for values in args.strand]
        decision = decide(version, strands, args.buf)
    except (InputError, OSError) as error:
        print(f"::error title=Schema version::{error}")
        return 2

    text = report(decision)
    if decision.passed:
        print(text)
    elif args.enforce:
        print(f"::error title=Schema version::SCHEMA_VERSION must be {decision.required}")
        print(text)
    else:
        print(f"::warning title=Schema version::SCHEMA_VERSION must be {decision.required} before merging into main")
        print(text)

    if args.comment:
        with open(args.comment, "w") as f:
            f.write(comment(decision, strands, args.buf, args.account, args.enforce))
    if args.github_output:
        with open(args.github_output, "a") as f:
            f.write(f"required={decision.required}\n")
            f.write(f"breaking={'true' if decision.breaking else 'false'}\n")
            f.write(f"passed={'true' if decision.passed else 'false'}\n")
    return 1 if args.enforce and not decision.passed else 0


if __name__ == "__main__":
    sys.exit(main())
