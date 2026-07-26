#!/usr/bin/env python3
"""Find decoder artifacts without treating a file name as decoder evidence.

The output is deliberately conservative: `containsWhile` and `containsAdd34`
are byte-level observations, not proof that an artifact is runnable or that a
specific cache-state mapping is correct.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
from typing import Any


CANDIDATE_NAMES = re.compile(
    r"(decoder.*\.(?:onnx|ort|json|pdmodel|pir)|"
    r"inference\.(?:json|pdiparams|pdmodel)|"
    r".*formulanet.*\.(?:zip|tar|gz|json|pdiparams)|"
    r".*decoder.*\.(?:zip|tar|gz))$",
    re.IGNORECASE,
)
SKIP_DIRS = {".git", "node_modules", "__pycache__", ".venv", "venv"}
BYTE_SCAN_LIMIT = 512 * 1024 * 1024


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def inspect_bytes(data: bytes) -> tuple[bool, bool]:
    lowered = data.lower()
    return b"while" in lowered, any(
        marker in lowered for marker in (b"add.34", b"add_34", b"/add_34")
    )


def artifact_format(name: str, data: bytes | None = None) -> str:
    lowered = name.lower()
    if lowered.endswith(".onnx"):
        return "onnx"
    if lowered.endswith(".ort"):
        return "ort"
    if lowered.endswith(".pdiparams"):
        return "paddle-parameters"
    if lowered.endswith((".pdmodel", ".pir")):
        return "paddle-program"
    if lowered.endswith(".json"):
        if data and b'"magic":"pir"' in data.replace(b" ", b""):
            return "paddle-pir-json"
        return "json"
    if lowered.endswith((".zip", ".tar", ".gz", ".tgz")):
        return "archive"
    return "unknown"


def safe_path(
    path: pathlib.Path, roots: list[pathlib.Path], content_hash: str
) -> str:
    resolved = path.resolve()
    for index, root in enumerate(roots):
        try:
            relative = resolved.relative_to(root.resolve())
        except ValueError:
            continue
        if index != 0:
            return f"<search-root-{index}>/candidate-{content_hash[:12]}"
        prefix = "<repo>"
        suffix = relative.as_posix()
        return f"{prefix}/{suffix}" if suffix != "." else prefix
    return f"<external>/candidate-{content_hash[:12]}"


def local_candidates(roots: list[pathlib.Path]) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    seen: set[pathlib.Path] = set()
    for root in roots:
        if not root.exists():
            continue
        for current, directories, files in os.walk(root):
            directories[:] = [
                name
                for name in directories
                if name not in SKIP_DIRS
                and not (
                    name == "target"
                    and pathlib.Path(current).resolve() != root.resolve()
                )
            ]
            for name in files:
                if not CANDIDATE_NAMES.search(name):
                    continue
                path = (pathlib.Path(current) / name).resolve()
                if path in seen:
                    continue
                seen.add(path)
                size = path.stat().st_size
                data = path.read_bytes() if size <= BYTE_SCAN_LIMIT else None
                contains_while, contains_add34 = inspect_bytes(data or b"")
                content_hash = sha256_file(path)
                results.append(
                    {
                        "location": "local",
                        "path": safe_path(path, roots, content_hash),
                        "sizeBytes": size,
                        "sha256": content_hash,
                        "format": artifact_format(name, data),
                        "containsWhile": contains_while,
                        "containsAdd34": contains_add34,
                        "decoderStepNameMatch": bool(
                            re.search(r"decoder.*step", name, re.IGNORECASE)
                        ),
                    }
                )
    return results


def run_git(repo: pathlib.Path, args: list[str]) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["git", *args],
        cwd=repo,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def git_candidates(repo: pathlib.Path) -> list[dict[str, Any]]:
    listing = run_git(repo, ["rev-list", "--objects", "--all", "--reflog"])
    results: list[dict[str, Any]] = []
    seen: set[str] = set()
    for line in listing.stdout.splitlines():
        object_id, separator, raw_path = line.partition(b" ")
        if not separator:
            continue
        blob = object_id.decode("ascii", "ignore")
        path = raw_path.decode("utf-8", "replace")
        if blob in seen or not CANDIDATE_NAMES.search(path):
            continue
        if run_git(repo, ["cat-file", "-t", blob]).stdout.strip() != b"blob":
            continue
        seen.add(blob)
        content = run_git(repo, ["cat-file", "blob", blob]).stdout
        contains_while, contains_add34 = inspect_bytes(content)
        results.append(
            {
                "location": "git-object",
                "path": path,
                "gitObject": blob,
                "sizeBytes": len(content),
                "sha256": hashlib.sha256(content).hexdigest(),
                "format": artifact_format(path, content),
                "containsWhile": contains_while,
                "containsAdd34": contains_add34,
            }
        )
    return results


def gh_json(repo_slug: str, endpoint: str) -> Any:
    if not shutil.which("gh"):
        return None
    result = subprocess.run(
        ["gh", "api", endpoint],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode:
        return None
    return json.loads(result.stdout)


def github_candidates(repo_slug: str | None) -> list[dict[str, Any]]:
    if not repo_slug:
        return []
    results: list[dict[str, Any]] = []
    releases = gh_json(repo_slug, f"repos/{repo_slug}/releases?per_page=100") or []
    for release in releases:
        for asset in release.get("assets", []):
            name = asset.get("name", "")
            if not CANDIDATE_NAMES.search(name):
                continue
            results.append(
                {
                    "location": "github-release",
                    "path": f"{release.get('tag_name')}/{name}",
                    "url": asset.get("browser_download_url"),
                    "sizeBytes": asset.get("size"),
                    "sha256": None,
                    "format": artifact_format(name),
                    "containsWhile": None,
                    "containsAdd34": None,
                    "notDownloaded": True,
                }
            )
    artifacts = gh_json(
        repo_slug, f"repos/{repo_slug}/actions/artifacts?per_page=100"
    ) or {}
    for artifact in artifacts.get("artifacts", []):
        name = artifact.get("name", "")
        if not CANDIDATE_NAMES.search(name):
            continue
        results.append(
            {
                "location": "github-actions",
                "path": name,
                "artifactId": artifact.get("id"),
                "sizeBytes": artifact.get("size_in_bytes"),
                "expired": artifact.get("expired"),
                "sha256": None,
                "format": artifact_format(name),
                "containsWhile": None,
                "containsAdd34": None,
                "notDownloaded": True,
            }
        )
    return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=pathlib.Path, default=pathlib.Path.cwd())
    parser.add_argument("--search-root", action="append", type=pathlib.Path, default=[])
    parser.add_argument("--github-repo")
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    repo = args.repo_root.resolve()
    roots = [repo, *[path.resolve() for path in args.search_root]]
    candidates = [
        *local_candidates(roots),
        *git_candidates(repo),
        *github_candidates(args.github_repo),
    ]
    candidates.sort(
        key=lambda item: (
            item["location"],
            str(item["path"]).lower(),
            item.get("sha256") or "",
        )
    )
    decoder_steps = [
        item
        for item in candidates
        if item.get("decoderStepNameMatch")
        or re.search(r"decoder.*step", pathlib.Path(item["path"]).name, re.I)
    ]
    output = {
        "schemaVersion": 1,
        "repository": args.github_repo or repo.name,
        "searchRoots": [
            "<repo>",
            *[f"<search-root-{index}>" for index in range(1, len(roots))],
        ],
        "candidateCount": len(candidates),
        "decoderStepCandidateCount": len(decoder_steps),
        "decoderStepStatus": "found" if decoder_steps else "blocked",
        "limitations": [
            "GitHub metadata is inspected without downloading large archives.",
            "containsWhile/containsAdd34 are byte observations, not graph validation.",
            "A full decoder model is not treated as an incremental decoder_step model.",
        ],
        "candidates": candidates,
    }
    encoded = json.dumps(output, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    else:
        print(encoded, end="")
    return 0 if decoder_steps else 2


if __name__ == "__main__":
    raise SystemExit(main())
