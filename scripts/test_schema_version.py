"""Tests of the schema version decision in scripts/schema_version.py.

Run with ``python3 -m unittest discover -s scripts``. The decision is what makes the schema version rule-based rather
than chosen, so each test states the rule it holds the script to.
"""

import doctest
import os
import tempfile
import unittest

import schema_version
from schema_version import InputError, Strand, decide, main, plan_release

FIGURE = "ironlab-figure"
EDIT = "ironlab-edit"


def strands(figure, edit):
    """Build the two strands from (latest, suggested, change) triples."""
    return [Strand(FIGURE, *figure), Strand(EDIT, *edit)]


UNCHANGED = ("0.10.0", "0.10.0", "equal")
INITIAL = ("", "0.1.0", "initial")


class TheStrandsRequirement(unittest.TestCase):
    def test_unchanged_schemas_require_the_published_version(self):
        # Why: a pull request that does not touch the schema must keep SCHEMA_VERSION where it is.
        decision = decide("0.10.0", strands(UNCHANGED, UNCHANGED), "compatible")
        self.assertEqual(decision.required, "0.10.0")
        self.assertTrue(decision.passed)

    def test_an_equal_change_requires_the_latest_version_whatever_the_suggestion(self):
        # Why: the action's `version` output for an unchanged schema is not documented, so the requirement of an
        # equal strand must come from the published versions alone.
        decision = decide("0.10.0", strands(("0.10.0", "", "equal"), UNCHANGED), "compatible")
        self.assertEqual(decision.required, "0.10.0")

    def test_the_highest_suggestion_over_the_strands_wins(self):
        # Why: one schema version covers both documents, so it must satisfy the strand that changed most.
        decision = decide("0.10.1", strands(("0.10.0", "0.10.1", "patch"), ("0.10.0", "0.11.0", "minor")), "compatible")
        self.assertEqual(decision.required, "0.11.0")
        self.assertFalse(decision.passed)
        self.assertEqual(decision.sources, [f"strands ({EDIT})"])

    def test_a_patch_suggestion_is_required_exactly(self):
        decision = decide("0.10.1", strands(("0.10.0", "0.10.1", "patch"), UNCHANGED), "compatible")
        self.assertTrue(decision.passed)
        self.assertFalse(decision.breaking)

    def test_a_version_above_the_requirement_fails(self):
        # Why: the version is exactly what the rules require, never one chosen higher by hand.
        decision = decide("0.12.0", strands(("0.10.0", "0.11.0", "minor"), UNCHANGED), "compatible")
        self.assertFalse(decision.passed)
        self.assertEqual(decision.required, "0.11.0")

    def test_a_major_change_is_breaking(self):
        # Why: a breaking change reported by strands must be declared in a commit, like one reported by buf.
        decision = decide("0.11.0", strands(("0.10.0", "0.11.0", "major"), UNCHANGED), "compatible")
        self.assertTrue(decision.passed)
        self.assertTrue(decision.breaking)

    def test_a_minor_change_is_not_breaking(self):
        decision = decide("0.11.0", strands(("0.10.0", "0.11.0", "minor"), UNCHANGED), "compatible")
        self.assertFalse(decision.breaking)


class TheFirstPublication(unittest.TestCase):
    def test_initial_strands_require_schema_version_itself(self):
        # Why: with nothing published there is nothing to compare against, so the version of the schema as it stands
        # is published as the baseline. This is how the release of 0.18.1 publishes schema version 0.10.0.
        decision = decide("0.10.0", strands(INITIAL, INITIAL), "compatible")
        self.assertEqual(decision.required, "0.10.0")
        self.assertTrue(decision.passed)

    def test_the_initial_suggestion_of_strands_is_not_required(self):
        # Why: strands suggests 0.1.0 or 1.0.0 for a first publication, and requiring either would rewind or
        # prematurely stabilise a schema that already has a version history.
        for suggestion in ("0.1.0", "1.0.0"):
            with self.subTest(suggestion=suggestion):
                decision = decide("0.10.0", strands(("", suggestion, "initial"), ("", suggestion, "initial")), "compatible")
                self.assertEqual(decision.required, "0.10.0")
                self.assertTrue(decision.passed)

    def test_a_breaking_proto_change_with_nothing_published_still_requires_schema_version(self):
        # Why: a breaking bump needs a published version to bump; without one the first publication stands.
        decision = decide("0.10.0", strands(INITIAL, INITIAL), "breaking")
        self.assertEqual(decision.required, "0.10.0")
        self.assertTrue(decision.passed)
        self.assertTrue(decision.breaking)

    def test_an_initial_strand_beside_a_published_one_cannot_rewind_the_version(self):
        # Why: the strands carry the same version, so a strand created late must not allow a version below the one
        # already published to its partner.
        decision = decide("0.9.0", strands(INITIAL, UNCHANGED), "compatible")
        self.assertEqual(decision.required, "0.10.0")
        self.assertFalse(decision.passed)


class LockstepStrands(unittest.TestCase):
    def test_unchanged_strands_that_lag_require_the_highest_published_version(self):
        # Why: both strands are published at one version, so the version must never fall below either of them.
        decision = decide("0.11.0", strands(UNCHANGED, ("0.11.0", "0.11.0", "equal")), "compatible")
        self.assertEqual(decision.required, "0.11.0")
        self.assertTrue(decision.passed)

    def test_a_change_to_a_lagging_strand_bumps_the_highest_published_version(self):
        # Why: a patch change to the figure strand at 0.10.0 suggests 0.10.1, but the edit strand already holds
        # 0.11.0, and publishing both at 0.10.1 would go backwards; the same patch bump applies to 0.11.0.
        decision = decide("0.11.1", strands(("0.10.0", "0.10.1", "patch"), ("0.11.0", "0.11.0", "equal")), "compatible")
        self.assertEqual(decision.required, "0.11.1")
        self.assertTrue(decision.passed)


class TheBufRequirement(unittest.TestCase):
    def test_a_breaking_proto_change_bumps_the_minor_version_before_1(self):
        # Why: the JSON Schema may not show a change that breaks the Protocol Buffers encoding, such as a renumbered
        # field, so buf alone must be able to demand the breaking bump.
        decision = decide("0.10.0", strands(UNCHANGED, UNCHANGED), "breaking")
        self.assertEqual(decision.required, "0.11.0")
        self.assertEqual(decision.sources, ["buf breaking"])
        self.assertTrue(decision.breaking)
        self.assertFalse(decision.passed)

    def test_a_breaking_proto_change_bumps_the_major_version_from_1(self):
        stable = ("1.2.3", "1.2.3", "equal")
        decision = decide("2.0.0", strands(stable, stable), "breaking")
        self.assertEqual(decision.required, "2.0.0")
        self.assertTrue(decision.passed)

    def test_the_bump_starts_from_the_highest_published_version(self):
        # Why: the strands may lag one another, and the new version must exceed every published version.
        decision = decide("0.12.0", strands(("0.10.0", "0.10.0", "equal"), ("0.11.0", "0.11.0", "equal")), "breaking")
        self.assertEqual(decision.required, "0.12.0")
        self.assertTrue(decision.passed)

    def test_the_higher_of_the_two_requirements_wins(self):
        # Why: buf and strands may both find a change; the version must satisfy both.
        decision = decide("0.11.0", strands(("0.10.0", "0.10.1", "patch"), UNCHANGED), "breaking")
        self.assertEqual(decision.required, "0.11.0")
        self.assertTrue(decision.passed)

    def test_agreeing_tools_are_both_named(self):
        decision = decide("0.10.0", strands(("0.10.0", "0.11.0", "major"), UNCHANGED), "breaking")
        self.assertEqual(decision.sources, [f"strands ({FIGURE})", "buf breaking"])

    def test_a_compatible_or_skipped_buf_adds_no_requirement(self):
        for buf in ("compatible", "skipped"):
            with self.subTest(buf=buf):
                decision = decide("0.10.0", strands(UNCHANGED, UNCHANGED), buf)
                self.assertEqual(decision.required, "0.10.0")
                self.assertFalse(decision.breaking)


class TheStepToStable(unittest.TestCase):
    def test_1_0_0_passes_from_0_x_whatever_the_rules_require(self):
        # Why: the step to 1.0.0 is the one version that is chosen rather than computed.
        decision = decide("1.0.0", strands(("0.10.0", "0.10.1", "patch"), UNCHANGED), "compatible")
        self.assertTrue(decision.passed)
        self.assertTrue(decision.stable_step)
        self.assertEqual(decision.required, "0.10.1")

    def test_1_0_0_passes_from_0_x_without_any_change(self):
        self.assertTrue(decide("1.0.0", strands(UNCHANGED, UNCHANGED), "compatible").passed)

    def test_1_0_0_is_no_exception_once_stable(self):
        stable = ("1.0.0", "1.1.0", "minor")
        decision = decide("1.0.0", strands(stable, ("1.0.0", "1.0.0", "equal")), "compatible")
        self.assertFalse(decision.passed)

    def test_only_exactly_1_0_0_is_the_exception(self):
        for version in ("1.0.1", "1.1.0", "2.0.0", "1.0.0-rc.1"):
            with self.subTest(version=version):
                self.assertFalse(decide(version, strands(UNCHANGED, UNCHANGED), "compatible").passed)

    def test_1_0_0_with_nothing_published_is_a_first_publication_not_a_step(self):
        # Why: with nothing published, any SCHEMA_VERSION is required as itself, so 1.0.0 passes by the
        # first-publication rule and is not reported as the exception.
        decision = decide("1.0.0", strands(INITIAL, INITIAL), "compatible")
        self.assertTrue(decision.passed)
        self.assertFalse(decision.stable_step)


class TheReleasePlan(unittest.TestCase):
    def test_nothing_is_published_when_both_schemas_are_unchanged(self):
        # Why: a merge that leaves the schema alone must not publish a version that the strands already hold.
        self.assertEqual(plan_release("0.10.0", strands(UNCHANGED, UNCHANGED)), [])

    def test_a_change_to_one_strand_publishes_both(self):
        # Why: the strands carry the same version, so an unchanged strand is published again beside the changed one.
        self.assertEqual(plan_release("0.10.1", strands(("0.10.0", "0.10.1", "patch"), UNCHANGED)), [FIGURE, EDIT])

    def test_a_first_publication_publishes_both_at_schema_version(self):
        self.assertEqual(plan_release("0.10.0", strands(INITIAL, INITIAL)), [FIGURE, EDIT])

    def test_a_strand_that_already_holds_schema_version_is_left_alone(self):
        # Why: when a release publishes one strand and fails on the other, re-running it must publish only the
        # remainder rather than fail on the version already published.
        self.assertEqual(plan_release("0.10.0", strands(INITIAL, UNCHANGED)), [FIGURE])
        published = ("0.10.1", "0.10.1", "equal")
        self.assertEqual(plan_release("0.10.1", strands(published, ("0.10.0", "0.10.1", "patch"))), [EDIT])

    def test_a_version_that_does_not_exceed_a_published_one_is_refused(self):
        # Why: such a version means that the check of the pull request was bypassed, as it is for a fork, and the
        # release must fail visibly rather than ask strands for a version it already holds.
        with self.assertRaises(InputError):
            plan_release("0.10.0", strands(("0.10.0", "0.10.1", "patch"), UNCHANGED))


class Inputs(unittest.TestCase):
    def test_malformed_inputs_are_refused(self):
        # Why: a missing or garbled output of the action must stop the check rather than pass it by accident.
        cases = [
            lambda: decide("0.10", strands(UNCHANGED, UNCHANGED), "compatible"),
            lambda: decide("0.10.0", strands(UNCHANGED, UNCHANGED), "maybe"),
            lambda: decide("0.10.0", [], "compatible"),
            lambda: Strand(FIGURE, "0.10.0", "0.11.0", "feature"),
            lambda: Strand(FIGURE, "", "0.11.0", "minor"),
            lambda: Strand(FIGURE, "0.10.0", "", "minor"),
            lambda: Strand(FIGURE, "None", "0.10.0", "equal"),
            lambda: Strand(FIGURE, "0.10.0", "0.10.0", "patch"),
            lambda: Strand(FIGURE, "0.10.0", "0.11.0-rc.1", "minor"),
            lambda: Strand(FIGURE, "0.10.0", "0.1.0", "initial"),
        ]
        for index, case in enumerate(cases):
            with self.subTest(case=index):
                with self.assertRaises(InputError):
                    case()


class CommandLine(unittest.TestCase):
    def run_check(self, current, figure, edit, buf, *extra):
        with tempfile.TemporaryDirectory() as tmp:
            output = os.path.join(tmp, "output")
            body = os.path.join(tmp, "comment.md")
            argv = ["check", "--current", current, "--strand", FIGURE, *figure, "--strand", EDIT, *edit, "--buf", buf]
            argv += ["--github-output", output, "--comment", body, *extra]
            status = main(argv)
            with open(output) as f:
                outputs = dict(line.split("=", 1) for line in f.read().splitlines())
            with open(body) as f:
                return status, outputs, f.read()

    def test_an_enforced_mismatch_fails_and_names_the_line_to_set(self):
        status, outputs, body = self.run_check(
            "0.10.0", ("0.10.0", "0.11.0", "minor"), UNCHANGED, "compatible", "--enforce"
        )
        self.assertEqual(status, 1)
        self.assertEqual(outputs, {"required": "0.11.0", "breaking": "false", "passed": "false"})
        self.assertIn('pub const SCHEMA_VERSION: &str = "0.11.0";', body)
        self.assertIn("https://strands.octue.com/octue/ironlab-figure", body)

    def test_an_unenforced_mismatch_only_reports(self):
        # Why: SCHEMA_VERSION is bumped on pull requests into main, so pull requests into a trunk report only.
        status, outputs, _ = self.run_check("0.10.0", UNCHANGED, UNCHANGED, "breaking")
        self.assertEqual(status, 0)
        self.assertEqual(outputs, {"required": "0.11.0", "breaking": "true", "passed": "false"})

    def test_a_match_passes(self):
        status, outputs, body = self.run_check("0.10.0", UNCHANGED, UNCHANGED, "compatible", "--enforce")
        self.assertEqual(status, 0)
        self.assertEqual(outputs["passed"], "true")
        self.assertIn("Passed.", body)

    def test_a_bad_input_exits_with_status_2(self):
        argv = ["check", "--current", "0.10.0", "--strand", FIGURE, "", "", "minor", "--strand", EDIT, *UNCHANGED]
        self.assertEqual(main([*argv, "--buf", "compatible"]), 2)

    def test_the_failure_report_states_the_version_the_tools_and_the_line(self):
        decision = decide("0.10.0", strands(("0.10.0", "0.10.1", "patch"), UNCHANGED), "breaking")
        text = schema_version.report(decision)
        self.assertIn("require 0.11.0", text)
        self.assertIn("buf breaking", text)
        self.assertIn("the schema has a patch change from published 0.10.0", text)
        self.assertIn('pub const SCHEMA_VERSION: &str = "0.11.0";', text)

    def test_the_release_command_writes_one_publish_output_per_strand(self):
        # Why: the release workflow publishes each strand in a step of its own, conditioned on these outputs.
        with tempfile.TemporaryDirectory() as tmp:
            output = os.path.join(tmp, "output")
            argv = ["release", "--current", "0.10.0", "--strand", FIGURE, *INITIAL, "--strand", EDIT, *UNCHANGED]
            self.assertEqual(main([*argv, "--github-output", output]), 0)
            with open(output) as f:
                outputs = dict(line.split("=", 1) for line in f.read().splitlines())
        self.assertEqual(
            outputs, {"version": "0.10.0", f"publish-{FIGURE}": "true", f"publish-{EDIT}": "false"}
        )

    def test_a_refused_release_exits_with_status_2(self):
        argv = ["release", "--current", "0.10.0", "--strand", FIGURE, "0.10.0", "0.10.1", "patch"]
        self.assertEqual(main([*argv, "--strand", EDIT, *UNCHANGED]), 2)


def load_tests(loader, tests, ignore):
    tests.addTests(doctest.DocTestSuite(schema_version))
    return tests


if __name__ == "__main__":
    unittest.main()
