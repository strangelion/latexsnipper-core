#!/usr/bin/env python3
"""Run the runtime-quality gates and emit a privacy-safe evidence bundle."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import pathlib
import platform
import re
import subprocess
import time
from typing import Any


TEST_RESULT = re.compile(
    r"test result: (?P<status>ok|FAILED)\. "
    r"(?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored; (?P<measured>\d+) measured; "
    r"(?P<filtered>\d+) filtered out"
)
PRIVATE_PATH = re.compile(
    r"(?i)(?:[a-z]:[\\/])?users[\\/][^\\/\s:]+[\\/]"
)


def run(repo: pathlib.Path, command: list[str]) -> dict[str, Any]:
    started = time.perf_counter()
    process = subprocess.run(
        command,
        cwd=repo,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
        env={**os.environ, "CARGO_TERM_COLOR": "never"},
    )
    duration = time.perf_counter() - started
    matches = list(TEST_RESULT.finditer(process.stdout))
    warnings = len(re.findall(r"(?m)^warning:", process.stdout))
    result: dict[str, Any] = {
        "command": command,
        "exitCode": process.returncode,
        "durationSeconds": round(duration, 3),
        "testSuites": len(matches),
        "testsPassed": sum(int(match["passed"]) for match in matches),
        "testsFailed": sum(int(match["failed"]) for match in matches),
        "testsIgnored": sum(int(match["ignored"]) for match in matches),
        "testsFiltered": sum(int(match["filtered"]) for match in matches),
        "compilerWarnings": warnings,
    }
    if process.returncode:
        tail = "\n".join(process.stdout.splitlines()[-40:])
        result["failureTail"] = PRIVATE_PATH.sub("<user-dir>/", tail)
    return result


def command_output(repo: pathlib.Path, command: list[str]) -> str:
    return subprocess.run(
        command,
        cwd=repo,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=True,
    ).stdout.strip()


def file_sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=pathlib.Path, default=pathlib.Path.cwd())
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    repo = args.repo_root.resolve()
    output = args.output if args.output.is_absolute() else repo / args.output
    suite = [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "check", "--locked", "--workspace", "--all-targets"],
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "latexsnipper-ast",
            "-p",
            "latexsnipper-api-types",
            "-p",
            "latexsnipper-artifact",
            "-p",
            "latexsnipper-inference",
            "-p",
            "latexsnipper-pipeline",
            "-p",
            "latexsnipper-runtime",
            "-p",
            "latexsnipper-engine",
            "-p",
            "latexsnipper-benchmark",
        ],
        [
            "cargo",
            "check",
            "--locked",
            "-p",
            "latexsnipper-runtime",
            "--example",
            "mmap_lifecycle",
            "--features",
            "runtime-mmap-experimental",
        ],
        [
            "cargo",
            "clippy",
            "--locked",
            "-p",
            "latexsnipper-ast",
            "-p",
            "latexsnipper-api-types",
            "-p",
            "latexsnipper-artifact",
            "-p",
            "latexsnipper-inference",
            "-p",
            "latexsnipper-pipeline",
            "-p",
            "latexsnipper-runtime",
            "-p",
            "latexsnipper-engine",
            "-p",
            "latexsnipper-benchmark",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ]
    started = time.perf_counter()
    commands = [run(repo, command) for command in suite]
    report_paths = sorted(
        [
            *repo.glob("docs/reports/*2026-07-27.json"),
            *repo.glob("benchmarks/*/v1/report-*.json"),
            *repo.glob("benchmarks/*/v1/predictions-*.json"),
        ],
        key=lambda path: path.as_posix(),
    )
    source_commit = command_output(repo, ["git", "rev-parse", "HEAD"])
    tests = {
        "passed": sum(command["testsPassed"] for command in commands),
        "failed": sum(command["testsFailed"] for command in commands),
        "ignored": sum(command["testsIgnored"] for command in commands),
        "filtered": sum(command["testsFiltered"] for command in commands),
    }
    clippy_warnings = sum(command["compilerWarnings"] for command in commands)
    evidence = {
        "schemaVersion": 1,
        "generatedAtUtc": dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "commit": source_commit,
        "sourceCommit": source_commit,
        "worktreeDirty": bool(command_output(repo, ["git", "status", "--porcelain"])),
        "platform": {
            "os": platform.system(),
            "release": platform.release(),
            "architecture": platform.machine(),
            "python": platform.python_version(),
            "rustc": command_output(repo, ["rustc", "--version"]),
            "cargo": command_output(repo, ["cargo", "--version"]),
        },
        "commands": commands,
        "tests": tests,
        "clippyWarnings": clippy_warnings,
        "summary": {
            "passed": all(command["exitCode"] == 0 for command in commands),
            "durationSeconds": round(time.perf_counter() - started, 3),
            "testsPassed": tests["passed"],
            "testsFailed": tests["failed"],
            "testsIgnored": tests["ignored"],
            "testsFiltered": tests["filtered"],
            "compilerWarnings": clippy_warnings,
        },
        "artifacts": [
            {
                "path": path.relative_to(repo).as_posix(),
                "sizeBytes": path.stat().st_size,
                "sha256": file_sha256(path),
            }
            for path in report_paths
        ],
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    return 0 if evidence["summary"]["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
