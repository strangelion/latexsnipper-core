#!/usr/bin/env python3
"""Require a complete cargo audit, not just a zero exit after registry errors."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
from pathlib import Path
import re
import subprocess
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def assess_audit(exit_code: int, stdout: str, stderr: str) -> dict[str, Any]:
    errors = sum(
        line.strip().startswith("error:")
        for line in ANSI.sub("", stderr).splitlines()
    )
    summary: dict[str, Any] = {
        "schemaVersion": 1,
        "passed": False,
        "cargoAuditExitCode": exit_code,
        "executionErrorCount": errors,
        "reportValid": False,
        "registryChecksComplete": False,
    }
    try:
        report = json.loads(stdout)
        database = report["database"]
        vulnerabilities = report["vulnerabilities"]
        warnings = report["warnings"]
        settings = report["settings"]
        valid = (
            isinstance(database["advisory-count"], int)
            and not isinstance(database["advisory-count"], bool)
            and database["advisory-count"] > 0
            and isinstance(database["last-commit"], str)
            and re.fullmatch(r"[0-9a-f]{40}", database["last-commit"]) is not None
            and isinstance(database["last-updated"], str)
            and isinstance(vulnerabilities["count"], int)
            and not isinstance(vulnerabilities["count"], bool)
            and vulnerabilities["count"] >= 0
            and isinstance(vulnerabilities["list"], list)
            and vulnerabilities["count"] == len(vulnerabilities["list"])
            and isinstance(vulnerabilities["found"], bool)
            and vulnerabilities["found"] == (vulnerabilities["count"] > 0)
            and isinstance(warnings, dict)
            and all(isinstance(group, list) for group in warnings.values())
            and isinstance(settings["ignore"], list)
            and all(
                isinstance(value, str) and re.fullmatch(r"RUSTSEC-\d{4}-\d{4}", value)
                for value in settings["ignore"]
            )
        )
        if not valid:
            return summary
        updated = dt.datetime.fromisoformat(database["last-updated"].replace("Z", "+00:00"))
        if updated.utcoffset() is None:
            return summary
        advisory_ids = [
            item["advisory"]["id"]
            for item in vulnerabilities["list"]
            + [item for group in warnings.values() for item in group]
        ]
        if not all(
            isinstance(value, str) and re.fullmatch(r"RUSTSEC-\d{4}-\d{4}", value)
            for value in advisory_ids
        ):
            return summary
        summary.update(
            reportValid=True,
            advisoryDatabase={
                key: database[key]
                for key in ("advisory-count", "last-commit", "last-updated")
            },
            registryChecksComplete=errors == 0,
            ignoredAdvisoryIds=settings["ignore"],
            actionableAdvisoryIds=sorted(set(advisory_ids)),
            passed=exit_code == 0 and errors == 0 and not advisory_ids,
        )
    except (ValueError, KeyError, TypeError):
        pass
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--db", type=Path, help="optional advisory database checkout")
    args = parser.parse_args()
    command = ["cargo", "audit", "--deny", "warnings", "--json"]
    if args.db is not None:
        command.extend(["--db", str(args.db)])
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
            env={**os.environ, "CARGO_TERM_COLOR": "never", "NO_COLOR": "1"},
        )
        summary = assess_audit(result.returncode, result.stdout, result.stderr)
    except OSError:
        summary = assess_audit(1, "", "error: cargo audit could not start")
    serialized = json.dumps(summary, ensure_ascii=True, indent=2) + "\n"
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(serialized, encoding="utf-8")
    print(serialized, end="")
    return 0 if summary["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
