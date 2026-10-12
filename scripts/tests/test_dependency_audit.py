from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_dependency_audit import assess_audit, main


def clean_report():
    return {
        "database": {
            "advisory-count": 1,
            "last-commit": "a" * 40,
            "last-updated": "2026-10-13T00:00:00Z",
        },
        "vulnerabilities": {"found": False, "count": 0, "list": []},
        "warnings": {},
        "settings": {"ignore": ["RUSTSEC-2026-0009"]},
    }


class DependencyAuditTests(unittest.TestCase):
    def test_runner_keeps_normal_checks_and_returns_success_for_complete_results(self):
        result = subprocess.CompletedProcess([], 0, json.dumps(clean_report()), "")
        output = io.StringIO()
        with patch("sys.argv", ["verify_dependency_audit.py"]), patch(
            "verify_dependency_audit.subprocess.run", return_value=result
        ) as run, redirect_stdout(output):
            self.assertEqual(main(), 0)
        command = run.call_args.args[0]
        self.assertEqual(command, ["cargo", "audit", "--deny", "warnings", "--json"])
        self.assertTrue(json.loads(output.getvalue())["passed"])

    def test_runner_propagates_incomplete_zero_exit_as_a_failure(self):
        result = subprocess.CompletedProcess(
            [], 0, json.dumps(clean_report()), "error: registry query timed out\n"
        )
        with patch("sys.argv", ["verify_dependency_audit.py"]), patch(
            "verify_dependency_audit.subprocess.run", return_value=result
        ), redirect_stdout(io.StringIO()):
            self.assertEqual(main(), 1)

    def test_complete_clean_report_passes_without_changing_exceptions(self):
        result = assess_audit(0, json.dumps(clean_report()), "")
        self.assertTrue(result["passed"])
        self.assertTrue(result["registryChecksComplete"])
        self.assertEqual(result["ignoredAdvisoryIds"], ["RUSTSEC-2026-0009"])

    def test_zero_exit_with_registry_errors_does_not_pass(self):
        stderr = "\x1b[31merror: couldn't check if the package is yanked: timeout\x1b[0m\n"
        result = assess_audit(0, json.dumps(clean_report()), stderr)
        self.assertFalse(result["passed"])
        self.assertFalse(result["registryChecksComplete"])
        self.assertEqual(result["executionErrorCount"], 1)

    def test_nonzero_exit_never_passes(self):
        self.assertFalse(assess_audit(1, json.dumps(clean_report()), "")["passed"])

    def test_missing_malformed_and_inconsistent_reports_fail_closed(self):
        for stdout in ["", "not JSON", "null", "[]", "{}"]:
            result = assess_audit(0, stdout, "")
            self.assertFalse(result["passed"])
            self.assertFalse(result["registryChecksComplete"])
        for update in [
            {"database": {}},
            {"vulnerabilities": {"found": False, "count": 1, "list": []}},
            {"warnings": {"unmaintained": "invalid"}},
            {"settings": {"ignore": None}},
        ]:
            report = clean_report()
            report.update(update)
            self.assertFalse(assess_audit(0, json.dumps(report), "")["passed"])

    def test_actionable_warnings_and_vulnerabilities_fail_even_with_zero_exit(self):
        report = clean_report()
        report["warnings"] = {"unmaintained": [{"advisory": {"id": "RUSTSEC-2024-0449"}}]}
        self.assertFalse(assess_audit(0, json.dumps(report), "")["passed"])
        report = clean_report()
        report["vulnerabilities"] = {
            "found": True,
            "count": 1,
            "list": [{"advisory": {"id": "RUSTSEC-2026-0316"}}],
        }
        self.assertFalse(assess_audit(0, json.dumps(report), "")["passed"])

    def test_warning_ids_and_database_identity_are_validated(self):
        report = clean_report()
        report["warnings"] = {"unmaintained": [{}]}
        self.assertFalse(assess_audit(0, json.dumps(report), "")["passed"])
        report = clean_report()
        report["database"]["last-commit"] = "unknown"
        self.assertFalse(assess_audit(0, json.dumps(report), "")["passed"])
        report = clean_report()
        report["database"]["last-updated"] = "not a timestamp"
        self.assertFalse(assess_audit(0, json.dumps(report), "")["passed"])
        report = clean_report()
        report["settings"]["ignore"] = ["not an advisory"]
        self.assertFalse(assess_audit(0, json.dumps(report), "")["passed"])


if __name__ == "__main__":
    unittest.main()
