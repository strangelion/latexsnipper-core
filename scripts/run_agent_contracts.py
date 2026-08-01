#!/usr/bin/env python3
"""Execute Core agent contracts and emit JSON, JUnit, and SHA-256 evidence.

Contract files use JSON syntax, which is a strict subset of YAML, so the runner
does not depend on a third-party YAML parser. CI always reruns commands and does
not trust an agent-authored status.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT_ROOT = ROOT / "contracts" / "agent"
ALL_SUITES = {
    "runtime",
    "provider",
    "ort128",
    "int8",
    "conversion",
    "office-output",
    "failure-corpus",
    "model-package-security",
    "drawing",
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_command(command: list[str]) -> tuple[str, str | None, list[str], int]:
    executable = shutil.which(command[0])
    if executable is None:
        return "notRun", f"required executable '{command[0]}' is unavailable", [], 0
    started = time.monotonic()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    duration = int((time.monotonic() - started) * 1000)
    evidence = completed.stdout.splitlines()[-80:]
    if completed.returncode == 0:
        return "passed", None, evidence, duration
    return (
        "failed",
        f"command exited with code {completed.returncode}",
        evidence,
        duration,
    )


def check_paths(paths: list[str]) -> tuple[str, str | None, list[str], int]:
    missing = [path for path in paths if not (ROOT / path).exists()]
    if missing:
        return "failed", f"missing paths: {', '.join(missing)}", [], 0
    return "passed", None, [str((ROOT / path).relative_to(ROOT)) for path in paths], 0


def check_hash_manifest(relative: str) -> tuple[str, str | None, list[str], int]:
    manifest_path = ROOT / relative
    if not manifest_path.is_file():
        return "failed", f"hash manifest is missing: {relative}", [], 0
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    failures: list[str] = []
    evidence: list[str] = []
    for item in manifest.get("artifacts", []):
        path = ROOT / item["path"]
        if not path.is_file():
            failures.append(f"missing {item['path']}")
            continue
        actual = sha256(path)
        evidence.append(f"{item['path']} sha256={actual}")
        if actual != item["sha256"].lower():
            failures.append(f"hash mismatch {item['path']}")
    if failures:
        return "failed", "; ".join(failures), evidence, 0
    return "passed", None, evidence, 0


def check_truthful_matrix(relative: str) -> tuple[str, str | None, list[str], int]:
    path = ROOT / relative
    matrix = json.loads(path.read_text(encoding="utf-8"))
    allowed = {"required", "unsupported", "notRun", "blocked"}
    bad = []
    unsupported = []
    for case in matrix.get("cases", []):
        status = case.get("ciStatus")
        if status not in allowed:
            bad.append(f"{case.get('id')}: invalid status {status}")
        if status in {"unsupported", "notRun", "blocked"}:
            unsupported.append(f"{case.get('id')}={status}")
        if case.get("fallbackReportedAsRequested", False):
            bad.append(f"{case.get('id')}: fallback is misreported as requested provider")
    if bad:
        return "failed", "; ".join(bad), unsupported, 0
    return "passed", None, unsupported or ["all cases are required"], 0


def execute(rule: dict) -> tuple[str, str | None, list[str], int]:
    evidence = rule.get("evidence", {})
    kind = evidence.get("kind", "command")
    if kind == "command":
        return run_command(evidence["command"])
    if kind == "paths":
        return check_paths(evidence["paths"])
    if kind == "hashManifest":
        return check_hash_manifest(evidence["manifest"])
    if kind == "truthfulMatrix":
        return check_truthful_matrix(evidence["matrix"])
    return "blocked", f"unknown evidence kind '{kind}'", [], 0


def load_rules(suite: str) -> list[dict]:
    rules: list[dict] = []
    for path in sorted(CONTRACT_ROOT.glob("*.v1.yaml")):
        document = json.loads(path.read_text(encoding="utf-8"))
        if document.get("contractVersion") != 1:
            raise ValueError(f"{path}: unsupported contractVersion")
        for rule in document.get("rules", []):
            if rule.get("suite") == suite:
                rule = dict(rule)
                rule["contractFile"] = path.relative_to(ROOT).as_posix()
                rules.append(rule)
    if not rules:
        raise ValueError(f"suite '{suite}' has no contract rules")
    return rules


def write_junit(path: Path, suite: str, checks: list[dict]) -> None:
    testsuite = ET.Element(
        "testsuite",
        name=f"contract-{suite}",
        tests=str(len(checks)),
        failures=str(sum(check["status"] == "failed" for check in checks)),
        skipped=str(sum(check["status"] in {"blocked", "notRun"} for check in checks)),
        time=f"{sum(check['durationMs'] for check in checks) / 1000:.3f}",
    )
    for check in checks:
        case = ET.SubElement(
            testsuite,
            "testcase",
            name=check["id"],
            classname=check["contractFile"],
            time=f"{check['durationMs'] / 1000:.3f}",
        )
        if check["status"] == "failed":
            failure = ET.SubElement(case, "failure", message=check["reason"] or "failed")
            failure.text = "\n".join(check["evidence"])
        elif check["status"] in {"blocked", "notRun"}:
            ET.SubElement(case, "skipped", message=check["reason"] or check["status"])
        output = ET.SubElement(case, "system-out")
        output.text = "\n".join(check["evidence"])
    path.write_bytes(ET.tostring(testsuite, encoding="utf-8", xml_declaration=True))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--suite", required=True, choices=sorted(ALL_SUITES))
    parser.add_argument("--output-dir", type=Path, default=ROOT / "artifacts" / "contracts")
    args = parser.parse_args()
    rules = load_rules(args.suite)
    checks = []
    for rule in rules:
        status, reason, evidence, duration = execute(rule)
        checks.append(
            {
                "id": rule["id"],
                "status": status,
                "severity": rule.get("severity", "blocking"),
                "contractFile": rule["contractFile"],
                "durationMs": duration,
                "evidence": evidence,
                "reason": reason,
            }
        )
    commit = os.environ.get("GITHUB_SHA")
    if not commit:
        completed = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True, check=False
        )
        commit = completed.stdout.strip() or "unknown"
    report = {
        "contractVersion": 1,
        "suite": args.suite,
        "commit": commit,
        "platform": {"os": sys.platform, "architecture": platform.machine() or "unknown"},
        "checks": checks,
        "claims": [],
        "unsupportedClaims": [
            {"id": check["id"], "status": check["status"], "reason": check["reason"]}
            for check in checks
            if check["status"] in {"blocked", "notRun"}
        ],
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    json_path = args.output_dir / f"contract-{args.suite}.json"
    junit_path = args.output_dir / f"contract-{args.suite}.junit.xml"
    sha_path = args.output_dir / f"contract-{args.suite}.sha256"
    json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    write_junit(junit_path, args.suite, checks)
    sha_path.write_text(
        f"{sha256(json_path)}  {json_path.name}\n{sha256(junit_path)}  {junit_path.name}\n",
        encoding="utf-8",
    )
    blocking_bad = [
        check
        for check in checks
        if check["severity"] == "blocking" and check["status"] != "passed"
    ]
    print(json.dumps(report, indent=2, sort_keys=True))
    return 1 if blocking_bad else 0


if __name__ == "__main__":
    raise SystemExit(main())
