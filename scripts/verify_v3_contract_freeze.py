#!/usr/bin/env python3
"""Verify deliberate source snapshots for frozen Core 3 public contracts."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "contracts" / "v3-contract-freeze.json"
CONTRACT_FILES = (
    "wit/plugin-v1/plugin.wit",
    "crates/api-types/src/v3.rs",
    "crates/plugin/src/manifest_v3.rs",
    "crates/plugin/src/signed_registry.rs",
    "crates/model/src/manifest_v3.rs",
    "crates/conversion/src/capability_registry.rs",
    "crates/cli/src/main.rs",
    "crates/wasm/js/src/types.ts",
    "crates/evaluation/src/schema.rs",
    "crates/evaluation/src/int8.rs",
    "crates/evaluation/src/failure_corpus.rs",
    "crates/fidelity/src/lib.rs",
    "contracts/agent/core-runtime-contract.v1.yaml",
    "contracts/agent/core-provider-contract.v1.yaml",
    "contracts/agent/core-quality-contract.v1.yaml",
    "contracts/agent/core-conversion-contract.v1.yaml",
    "contracts/agent/core-security-contract.v1.yaml",
    "contracts/agent/core-mixed-contract.v1.yaml",
    "contracts/agent/core-symbol-contract.v1.yaml",
    "quality/failure-corpus/candidate.schema.json",
    "quality/int8/thresholds.v1.json",
    "contracts/schema/drawing-office-payload-v1.schema.json",
    "contracts/schema/drawing-readiness-v1.schema.json",
    "contracts/fixtures/drawing-office-payload-v1.json",
    "contracts/fixtures/drawing-readiness-v1.json",
)
PUBLIC_RUST_TREES = (
    "crates/foundation/src",
    "crates/ast/src",
    "crates/tensor/src",
    "crates/model/src",
    "crates/image/src",
    "crates/runtime/src",
    "crates/syntax/src",
    "crates/export/src",
    "crates/conversion/src",
    "crates/inference/src",
    "crates/api-types/src",
    "crates/pipeline/src",
    "crates/engine/src",
    "crates/plugin/src",
    "crates/plugin-wasi/src",
    "crates/ffi/src",
    "crates/wasm/src",
    "crates/drawing/src",
    "crates/custom-symbols/src",
)


def digest(path: Path) -> str:
    normalized = path.read_bytes().replace(b"\r\n", b"\n")
    return hashlib.sha256(normalized).hexdigest()


def current_hashes() -> dict[str, str]:
    missing = [relative for relative in CONTRACT_FILES if not (ROOT / relative).is_file()]
    if missing:
        raise FileNotFoundError(", ".join(missing))
    return {relative: digest(ROOT / relative) for relative in CONTRACT_FILES}


def tree_digest(relative: str) -> str:
    directory = ROOT / relative
    files = sorted(path for path in directory.rglob("*.rs") if path.is_file())
    if not files:
        raise FileNotFoundError(relative)
    value = hashlib.sha256()
    for path in files:
        item = path.relative_to(ROOT).as_posix().encode("utf-8")
        content = path.read_bytes().replace(b"\r\n", b"\n")
        value.update(len(item).to_bytes(8, "big"))
        value.update(item)
        value.update(len(content).to_bytes(8, "big"))
        value.update(content)
    return value.hexdigest()


def current_tree_hashes() -> dict[str, str]:
    return {relative: tree_digest(relative) for relative in PUBLIC_RUST_TREES}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--update",
        action="store_true",
        help="rewrite the reviewed freeze manifest after an intentional contract change",
    )
    args = parser.parse_args()
    hashes = current_hashes()
    tree_hashes = current_tree_hashes()

    if args.update:
        MANIFEST.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "schemaVersion": 1,
            "contractRelease": "3.0.0",
            "hashAlgorithm": "sha256-normalized-lf",
            "files": hashes,
            "publicRustSourceTrees": tree_hashes,
        }
        MANIFEST.write_text(
            json.dumps(payload, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        print(f"updated {MANIFEST.relative_to(ROOT)}")
        return

    if not MANIFEST.is_file():
        print("contract freeze manifest is missing", file=sys.stderr)
        raise SystemExit(1)
    expected = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if expected.get("schemaVersion") != 1:
        print("unsupported contract freeze manifest schema", file=sys.stderr)
        raise SystemExit(1)
    if expected.get("contractRelease") != "3.0.0":
        print("contract freeze release must be 3.0.0", file=sys.stderr)
        raise SystemExit(1)
    expected_files = expected.get("files")
    expected_trees = expected.get("publicRustSourceTrees")
    failed = expected_files != hashes or expected_trees != tree_hashes
    if expected_files != hashes:
        all_paths = sorted(set(expected_files or {}) | set(hashes))
        for relative in all_paths:
            before = (expected_files or {}).get(relative)
            after = hashes.get(relative)
            if before != after:
                print(
                    f"contract changed: {relative} ({before or 'missing'} -> {after or 'missing'})",
                    file=sys.stderr,
                )
    if expected_trees != tree_hashes:
        all_trees = sorted(set(expected_trees or {}) | set(tree_hashes))
        for relative in all_trees:
            before = (expected_trees or {}).get(relative)
            after = tree_hashes.get(relative)
            if before != after:
                print(
                    f"public Rust source tree changed: {relative} "
                    f"({before or 'missing'} -> {after or 'missing'})",
                    file=sys.stderr,
                )
    if failed:
        print(
            "update the freeze manifest only with explicit contract review: "
            "python scripts/verify_v3_contract_freeze.py --update",
            file=sys.stderr,
        )
        raise SystemExit(1)
    print(
        f"verified {len(hashes)} contract files and "
        f"{len(tree_hashes)} public Rust source trees"
    )


if __name__ == "__main__":
    main()
