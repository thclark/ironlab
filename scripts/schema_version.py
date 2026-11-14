#!/usr/bin/env python3
"""Decide the schema version that the rule-based tools require, and which strands a release publishes.

The schema version is never chosen by a person. Two tools decide it:

- strands.octue.com compares each single-file JSON Schema (``ironlab-figure`` and ``ironlab-edit``) with the latest
  version published to its strand and suggests the next version.
- ``buf breaking`` compares the committed Protocol Buffers files with those of ``main``.

The two strands are published in lockstep: both always carry the same version, which is SCHEMA_VERSION. The
requirement is therefore based on the highest version published to either strand (the *baseline*):

- a strand whose schema is unchanged (``equal``) requires the baseline;
- a strand whose schema changed (``patch``, ``minor`` or ``major``) requires the bump that strands suggests for it,
  applied to the baseline; when the strands agree on their latest version, this is exactly the suggestion;
- a strand with no published version (``initial``) requires SCHEMA_VERSION itself, because there is nothing to compare
  against; the initial suggestion of strands (which may be 0.1.0 or 1.0.0) is not required;
- a breaking change reported by ``buf breaking`` requires the breaking bump of the baseline: the next minor version
  while the major version is 0, and the next major version otherwise.

The required version is the highest of these requirements, and the check passes when SCHEMA_VERSION equals it. The
single exception is the deliberate step to stable: SCHEMA_VERSION set to exactly 1.0.0 while the baseline is 0.x
passes.

On release, both strands are published at SCHEMA_VERSION unless both are ``equal``. A strand that already holds
SCHEMA_VERSION with an equal schema is left alone, which lets a release whose publication failed part way be re-run.

Usage (from CI; see .github/workflows/ci.yml and .github/workflows/release.yml):

    scripts/schema_version.py check --current VERSION \\
        --strand ironlab-figure LATEST SUGGESTED CHANGE \\
        --strand ironlab-edit LATEST SUGGESTED CHANGE \\
        --buf breaking|compatible|skipped [--enforce] [--comment PATH] [--github-output PATH]

    scripts/schema_version.py release --current VERSION \\
        --strand ironlab-figure LATEST SUGGESTED CHANGE \\
        --strand ironlab-edit LATEST SUGGESTED CHANGE [--github-output PATH]

VERSION is SCHEMA_VERSION as ``generate-schema --print-version`` prints it. LATEST, SUGGESTED and CHANGE are the
``latest_version``, ``version`` and ``change`` outputs of octue/publish-strand-version in suggest-only mode; LATEST is
empty for a strand with no published version.

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


def bump_between(latest, suggested):
    """Return the component (``major``, ``minor`` or ``patch``) that ``suggested`` bumps from ``latest``.

    >>> bump_between("0.10.0", "0.11.0"), bump_between("0.10.0", "0.10.1"), bump_between("1.2.3", "2.0.0")
    ('minor', 'patch', 'major')
    """
    old, new = parse(latest), parse(suggested)
    if new[3:] != (True, ""):
        raise InputError(f"suggested version {suggested!r} is a pre-release, which the rules do not use")
    if new[:3] <= old[:3]:
        raise InputError(f"suggested version {suggested} does not exceed published {latest}")
    if new[0] != old[0]:
        return "major"
    if new[1] != old[1]:
        return "minor"
    return "patch"


def apply_bump(version, component):
    """Return ``version`` with ``component`` bumped and every lower component reset.

    >>> apply_bump("0.11.0", "patch"), apply_bump("0.11.2", "minor"), apply_bump("1.2.3", "major")
    ('0.11.1', '0.12.0', '2.0.0')
    """
    major, minor, patch, _, _ = parse(version)
    if component == "major":
        return f"{major + 1}.0.0"
    if component == "minor":
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


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
        if self.change == "initial":
            if self.latest:
                raise InputError(f"strand {self.name}: an initial change cannot have a published version")
            return
        if not self.latest:
            raise InputError(f"strand {self.name}: a {self.change} change needs the latest published version")
        parse(self.latest)
        if self.change != "equal":
            try:
                bump_between(self.latest, self.suggested)
            except InputError as error:
                raise InputError(f"strand {self.name}: {error}") from None

    def requirement(self, current, baseline):
        """Return the version this strand requires, given SCHEMA_VERSION and the highest published version."""
        if self.change == "initial":
            return current
        if self.change == "equal":
            return baseline
        return apply_bump(baseline, bump_between(self.latest, self.suggested))

    def reason(self, baseline):
        if self.change == "initial":
            return "the strand has no published version, so SCHEMA_VERSION is published as its first version"
        if self.change == "equal":
            if self.latest == baseline:
                return f"the schema equals published {self.latest}"
            return f"the schema equals published {self.latest}, and {baseline} is published to another strand"
        component = bump_between(self.latest, self.suggested)
        if self.latest == baseline:
            return f"the schema has a {self.change} change from published {self.latest}"
        return (
            f"the schema has a {self.change} change from published {self.latest}, which takes a {component} bump "
            f"of {baseline}, the highest version published to either strand"
        )


@dataclass
class Decision:
    """The required version, how it was reached, and whether SCHEMA_VERSION meets it."""

    current: str
    required: str
    baseline: str | None
    breaking: bool
    passed: bool
    stable_step: bool
    reasons: list = field(default_factory=list)
    sources: list = field(default_factory=list)


def _check_inputs(current, strands):
    parse(current)
    if not strands:
        raise InputError("at least one strand is needed")


def decide(current, strands, buf):
    """Decide the required version from the strands' suggestions and the result of ``buf breaking``.

    ``current`` is SCHEMA_VERSION, ``strands`` a list of Strand, and ``buf`` one of "breaking", "compatible" or
    "skipped" (there are no files of main to compare against).
    """
    _check_inputs(current, strands)
    if buf not in BUF_RESULTS:
        raise InputError(f"buf result {buf!r} is not one of {', '.join(BUF_RESULTS)}")

    baseline = highest(s.latest for s in strands)
    candidates = []  # (version, source, reason)
    for strand in strands:
        candidates.append((strand.requirement(current, baseline), f"strands ({strand.name})", strand.reason(baseline)))

    buf_breaking = buf == "breaking"
    if buf_breaking:
        if baseline:
            candidates.append(
                (
                    breaking_bump(baseline),
                    "buf breaking",
                    f"the .proto files break compatibility with main, so published {baseline} takes a breaking bump",
                )
            )
        else:
            candidates.append(
                (None, "buf breaking", "the .proto files break compatibility with main, but nothing is published to bump")
            )

    required = highest(version for version, _, _ in candidates)
    sources = [source for version, source, _ in candidates if version == required]
    reasons = [(source, version, reason) for version, source, reason in candidates]

    breaking = buf_breaking or any(s.change == "major" for s in strands)
    stable_step = current == STABLE and baseline is not None and parse(baseline)[0] == 0
    passed = current == required or stable_step
    return Decision(current, required, baseline, breaking, passed, stable_step and current != required, reasons, sources)


def plan_release(current, strands):
    """Return the names of the strands that a release publishes at SCHEMA_VERSION.

    The strands are published in lockstep: nothing is published when every strand is ``equal``, and otherwise every
    strand is published, except one that already holds SCHEMA_VERSION with an equal schema. SCHEMA_VERSION must
    exceed the latest version of every strand it is published to; a version that does not means that the check of the
    pull request was bypassed, and publishing is refused rather than attempted.
    """
    _check_inputs(current, strands)
    if all(s.change == "equal" for s in strands):
        return []
    publish = [s for s in strands if not (s.change == "equal" and s.latest == current)]
    for strand in publish:
        if strand.latest and parse(current) <= parse(strand.latest):
            raise InputError(
                f"strand {strand.name}: SCHEMA_VERSION {current} does not exceed published {strand.latest}, "
                f"so it cannot be published"
            )
    return [s.name for s in publish]


def report(decision):
    """Return the text printed to the log: concise on success, and complete on failure."""
    if decision.passed:
        if decision.stable_step:
            return (
                f"SCHEMA_VERSION {decision.current} is the deliberate step to stable from published "
                f"{decision.baseline}; the rules alone would require {decision.required}."
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
    lines += ["| Strand | Published | Requires | Change |", "|---|---|---|---|"]
    for s in strands:
        link = f"[`{s.name}`]({STRANDS_URL}/{account}/{s.name})"
        requires = f"`{s.requirement(decision.current, decision.baseline)}`"
        published = f"`{s.latest}`" if s.latest else "—"
        change = f"**{s.change}**" if s.change == "major" else s.change
        lines.append(f"| {link} | {published} | {requires} | {change} |")
    buf_text = {
        "breaking": "**breaking**: the `.proto` files are incompatible with those of `main`",
        "compatible": "compatible with `main`",
        "skipped": "skipped, because there are no `.proto` files of `main` to compare against",
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
        lines += [
            "",
            "> [!WARNING]",
            "> The required change is breaking, so a commit must declare it with a `BREAKING-CHANGE:` paragraph.",
        ]
    lines += ["", "<sub>Updated by the `schema` job of `.github/workflows/ci.yml` on every push.</sub>"]
    return "\n".join(lines) + "\n"


def _add_common_arguments(parser):
    parser.add_argument("--current", required=True, help="SCHEMA_VERSION, as generate-schema --print-version prints it")
    parser.add_argument(
        "--strand", nargs=4, action="append", required=True, metavar=("NAME", "LATEST", "SUGGESTED", "CHANGE")
    )
    parser.add_argument("--github-output", help="append the outputs of the decision to this GitHub output file")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("check", help="check SCHEMA_VERSION against the version the rules require")
    _add_common_arguments(check)
    check.add_argument("--buf", required=True, choices=BUF_RESULTS)
    check.add_argument("--account", default=DEFAULT_ACCOUNT)
    check.add_argument("--enforce", action="store_true", help="exit with status 1 when the check fails")
    check.add_argument("--comment", help="write the Markdown of the pull request comment to this path")
    release = commands.add_parser("release", help="decide which strands a release publishes at SCHEMA_VERSION")
    _add_common_arguments(release)
    args = parser.parse_args(argv)

    try:
        version = args.current.strip()
        strands = [Strand(*values) for values in args.strand]
        if args.command == "release":
            publish = plan_release(version, strands)
        else:
            decision = decide(version, strands, args.buf)
    except InputError as error:
        print(f"::error title=Schema version::{error}")
        return 2

    if args.command == "release":
        if publish:
            print(f"Publish {', '.join(publish)} at {version}.")
        else:
            print("Every schema equals its published version, so nothing is published.")
        if args.github_output:
            with open(args.github_output, "a") as f:
                f.write(f"version={version}\n")
                for strand in strands:
                    f.write(f"publish-{strand.name}={'true' if strand.name in publish else 'false'}\n")
        return 0

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
