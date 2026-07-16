#!/usr/bin/env python3
"""Verify workspace/package version consistency and stable release tags."""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
STABLE_VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
INTERNAL_PREFIX = "latexsnipper-"


def load_toml(path: Path) -> dict:
    with path.open("rb") as source:
        return tomllib.load(source)


def fail(message: str) -> None:
    print(f"release-version error: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--expected")
    parser.add_argument("--tag")
    parser.add_argument("--stable", action="store_true")
    args = parser.parse_args()

    root_manifest = load_toml(ROOT / "Cargo.toml")
    version = root_manifest["workspace"]["package"]["version"]
    expected = args.expected or version
    if version != expected:
        fail(f"workspace version {version!r} does not match {expected!r}")
    if args.stable and not STABLE_VERSION.fullmatch(version):
        fail(f"release version {version!r} is not stable MAJOR.MINOR.PATCH")
    if args.tag is not None and args.tag != f"v{version}":
        fail(f"tag {args.tag!r} does not match workspace version v{version}")

    dependencies = root_manifest["workspace"].get("dependencies", {})
    for name, value in sorted(dependencies.items()):
        if not name.startswith(INTERNAL_PREFIX) or not isinstance(value, dict):
            continue
        dependency_version = value.get("version")
        if dependency_version is not None and dependency_version != version:
            fail(
                f"workspace dependency {name} uses {dependency_version!r}, "
                f"expected {version!r}"
            )

    for manifest_path in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        manifest = load_toml(manifest_path)
        package = manifest.get("package", {})
        package_version = package.get("version")
        if isinstance(package_version, str) and package_version != version:
            fail(
                f"{manifest_path.relative_to(ROOT)} uses {package_version!r}, "
                f"expected {version!r}"
            )

    npm_manifest_path = ROOT / "crates" / "wasm" / "js" / "package.json"
    npm_manifest = json.loads(npm_manifest_path.read_text(encoding="utf-8"))
    if npm_manifest["version"] != version:
        fail(
            f"{npm_manifest_path.relative_to(ROOT)} uses "
            f"{npm_manifest['version']!r}, expected {version!r}"
        )

    print(f"release version is consistent: {version}")


if __name__ == "__main__":
    main()
