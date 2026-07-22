#!/usr/bin/env python3
"""Prepare the official PP-FormulaNet-S full Paddle inference program.

This is a model-packaging tool, not a user runtime dependency. It downloads
the official PaddleX archive (or accepts an offline copy), verifies its fixed
SHA-256, and installs only the combined inference program and parameters.

Usage:
  python scripts/prepare_ppfn_paddle_inference.py
  python scripts/prepare_ppfn_paddle_inference.py --archive path/to/archive.tar
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import tarfile
import tempfile
import urllib.request
from pathlib import Path


ARCHIVE_URL = (
    "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/"
    "paddle3.0.0/PP-FormulaNet-S_infer.tar"
)
ARCHIVE_SHA256 = "a3ea2c005abdbe525d9e46c1e2e96021d900b0dd45535a112260d46dd437612e"
REQUIRED_FILES = ("inference.json", "inference.pdiparams")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def download(destination: Path) -> None:
    request = urllib.request.Request(
        ARCHIVE_URL,
        headers={"User-Agent": "LaTeXSnipper-model-packager/3"},
    )
    with urllib.request.urlopen(request) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output, length=1024 * 1024)


def matching_member(archive: tarfile.TarFile, filename: str) -> tarfile.TarInfo:
    matches = [
        member
        for member in archive.getmembers()
        if member.isfile() and Path(member.name).name == filename
    ]
    if len(matches) != 1:
        raise RuntimeError(
            f"expected exactly one {filename!r} in archive, found {len(matches)}"
        )
    return matches[0]


def install(archive_path: Path, target: Path) -> None:
    actual = sha256(archive_path)
    if actual.lower() != ARCHIVE_SHA256:
        raise RuntimeError(
            f"archive SHA-256 mismatch: expected {ARCHIVE_SHA256}, got {actual}"
        )

    target.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive_path, mode="r:*") as archive:
        for filename in REQUIRED_FILES:
            member = matching_member(archive, filename)
            source = archive.extractfile(member)
            if source is None:
                raise RuntimeError(f"could not read {member.name!r}")
            destination = target / filename
            temporary = target / f".{filename}.partial"
            with source, temporary.open("wb") as output:
                shutil.copyfileobj(source, output, length=1024 * 1024)
            temporary.replace(destination)
            print(f"installed {destination} ({destination.stat().st_size} bytes)")

    missing = [name for name in REQUIRED_FILES if not (target / name).is_file()]
    if missing:
        raise RuntimeError(f"model package remains incomplete; missing: {', '.join(missing)}")
    if not (target / "tokenizer.json").is_file():
        print(
            "warning: tokenizer.json is not present; install the shared PP-FormulaNet "
            "tokenizer before running inference"
        )


def parse_args() -> argparse.Namespace:
    repository = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="verified offline official tar archive")
    parser.add_argument(
        "--target",
        type=Path,
        default=repository / "models" / "formula-rec" / "pp-formulanet-s",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.archive:
        install(args.archive.resolve(), args.target.resolve())
        return
    with tempfile.TemporaryDirectory(prefix="latexsnipper-ppfn-") as directory:
        archive = Path(directory) / "PP-FormulaNet-S_infer.tar"
        print(f"downloading {ARCHIVE_URL}")
        download(archive)
        install(archive, args.target.resolve())


if __name__ == "__main__":
    main()
