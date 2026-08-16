#!/usr/bin/env python3
"""Verify the crates.io publish sequence in release.yml is a valid topo order.

cargo publish verifies each package against crates.io, where ALL
dependencies — including dev-dependencies, build-dependencies and optional
dependencies — must already resolve. A wrong order (e.g. publishing a crate
that dev-depends on a later crate) fails the whole release at that crate.

This script reads the full workspace graph from `cargo metadata` and checks
that the `publish latexsnipper-*` lines in .github/workflows/release.yml form
a topological order: every internal dependency of a crate must be published
before it.

Exit codes: 0 = valid order, 1 = order violation (release must be blocked).
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RELEASE_YML = ROOT / ".github" / "workflows" / "release.yml"
PUBLISH_PATTERN = re.compile(r"^\s*publish\s+(latexsnipper-[\w-]+)\s*$", re.MULTILINE)


def cargo_metadata() -> dict:
    try:
        result = subprocess.run(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=ROOT,
            capture_output=True,
            # Explicit UTF-8: on CJK Windows, cargo emits UTF-8 bytes that
            # text=True would try to decode with the locale codec (gbk).
            encoding="utf-8",
            errors="replace",
            check=True,
            timeout=120,
        )
        return json.loads(result.stdout)
    except subprocess.TimeoutExpired as error:
        print(
            "publish-order error: cargo metadata timed out; is a lockfile build in progress?",
            file=sys.stderr,
        )
        raise SystemExit(1) from error
    except subprocess.CalledProcessError as error:
        print(
            f"publish-order error: cargo metadata failed (exit {error.returncode}): "
            f"{error.stderr.strip()[-400:]}",
            file=sys.stderr,
        )
        raise SystemExit(1) from error
    except json.JSONDecodeError as error:
        print(
            f"publish-order error: cargo metadata returned invalid JSON: {error}",
            file=sys.stderr,
        )
        raise SystemExit(1) from error


def publish_sequence(text: str) -> list[str]:
    return PUBLISH_PATTERN.findall(text)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--workflow",
        default=str(RELEASE_YML),
        help="Path to the release workflow (default: release.yml)",
    )
    args = parser.parse_args()

    workflow_path = Path(args.workflow)
    if not workflow_path.exists():
        print(
            f"publish-order error: workflow not found: {workflow_path}", file=sys.stderr
        )
        raise SystemExit(1)

    sequence = publish_sequence(workflow_path.read_text(encoding="utf-8"))
    if not sequence:
        print(
            "publish-order error: no `publish latexsnipper-*` lines found",
            file=sys.stderr,
        )
        raise SystemExit(1)

    metadata = cargo_metadata()
    workspace = metadata["packages"]

    # Resolve workspace packages: a crate depends on an internal crate by name.
    packages_by_name = {p["name"]: p for p in workspace}
    internal = set(packages_by_name) & set(sequence)

    # All internal deps of a crate: dev, build, optional and normal.
    def internal_deps(name: str) -> set[str]:
        deps: set[str] = set()
        for dep in packages_by_name[name].get("dependencies", []):
            dep_name = dep.get("rename") or dep["name"]
            if dep_name in packages_by_name and dep_name in sequence:
                deps.add(dep_name)
        return deps

    # position lookup
    position = {name: index for index, name in enumerate(sequence)}

    # A crate must appear after every internal crate it depends on.
    violations: list[str] = []
    for name in sequence:
        for dep in sorted(internal_deps(name)):
            if position[dep] > position[name]:
                violations.append(
                    f"{name} is published at position {position[name]} but depends "
                    f"on {dep} at position {position[dep]}"
                )

    # Also fail if the sequence is missing any internal workspace crate that
    # other published crates depend on (a silently dropped crate would break
    # the graph downstream).
    for name in internal:
        for dep in sorted(internal_deps(name)):
            if dep not in sequence:
                violations.append(
                    f"{name} depends on {dep} which is not in the publish sequence"
                )

    if violations:
        print(
            "publish-order error: crates.io publish sequence is not a topological order",
            file=sys.stderr,
        )
        for violation in violations:
            print(f"  - {violation}", file=sys.stderr)
        print(
            "Reorder the `publish latexsnipper-*` lines in "
            f"{workflow_path.relative_to(ROOT)} so every dependency publishes first.",
            file=sys.stderr,
        )
        raise SystemExit(1)

    print(f"publish order is a valid topo order: {' -> '.join(sequence)}")


if __name__ == "__main__":
    main()
