#!/usr/bin/env python3
"""Build and package the LaTeXSnipper Paddle Inference runtime.

The input is an extracted official Paddle Inference SDK. The output is a flat,
redistributable runtime directory containing the versioned LaTeXSnipper C
bridge and Paddle's dynamic libraries. This is a developer/release tool; end
users do not need Python or a C++ compiler when the packaged directory ships
with the application.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


BRIDGE_ABI_VERSION = 1
BRIDGE_NAMES = {
    "win32": "latexsnipper_paddle_bridge.dll",
    "linux": "liblatexsnipper_paddle_bridge.so",
    "darwin": "liblatexsnipper_paddle_bridge.dylib",
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def dynamic_libraries(sdk: Path) -> list[Path]:
    if sys.platform == "win32":
        candidates = sdk.rglob("*.dll")
    elif sys.platform == "darwin":
        candidates = sdk.rglob("*.dylib")
    else:
        candidates = (
            path
            for path in sdk.rglob("*.so*")
            if ".so" in path.name
        )

    bridge_name = BRIDGE_NAMES[sys.platform]
    by_name: dict[str, Path] = {}
    hashes: dict[str, str] = {}
    for candidate in candidates:
        if not candidate.is_file() or candidate.name == bridge_name:
            continue
        key = candidate.name.casefold()
        candidate_hash = digest(candidate)
        if key in by_name and hashes[key] != candidate_hash:
            raise RuntimeError(
                "SDK contains different dynamic libraries with the same name: "
                f"{by_name[key]} and {candidate}"
            )
        by_name.setdefault(key, candidate)
        hashes.setdefault(key, candidate_hash)
    return [by_name[key] for key in sorted(by_name)]


def require_sdk(sdk: Path) -> None:
    header = sdk / "paddle" / "include" / "paddle_inference_api.h"
    library_dir = sdk / "paddle" / "lib"
    if not header.is_file() or not library_dir.is_dir():
        raise RuntimeError(
            f"{sdk} is not an extracted Paddle Inference SDK "
            "(expected paddle/include and paddle/lib)"
        )


def prepare_output(output: Path, force: bool) -> None:
    if output.exists() and any(output.iterdir()) and not force:
        raise RuntimeError(
            f"output directory is not empty: {output}; pass --force to overwrite files"
        )
    output.mkdir(parents=True, exist_ok=True)


def build_bridge(
    repository: Path,
    sdk: Path,
    build: Path,
    cmake: str,
    configuration: str,
    generator: str | None,
) -> Path:
    source = repository / "crates" / "runtime-paddle" / "native"
    install = build / "install"
    configure = [
        cmake,
        "-S",
        str(source),
        "-B",
        str(build),
        f"-DPADDLE_INFERENCE_ROOT={sdk}",
        f"-DCMAKE_INSTALL_PREFIX={install}",
    ]
    if generator:
        configure.extend(["-G", generator])
    subprocess.run(configure, check=True)
    subprocess.run(
        [cmake, "--build", str(build), "--config", configuration],
        check=True,
    )
    subprocess.run(
        [
            cmake,
            "--install",
            str(build),
            "--config",
            configuration,
            "--prefix",
            str(install),
        ],
        check=True,
    )

    name = BRIDGE_NAMES.get(sys.platform)
    if name is None:
        raise RuntimeError(f"unsupported packaging platform: {sys.platform}")
    matches = list(install.rglob(name))
    if len(matches) != 1:
        raise RuntimeError(f"expected one installed {name}, found {len(matches)}")
    return matches[0]


def package(sdk: Path, bridge: Path, output: Path) -> None:
    libraries = dynamic_libraries(sdk)
    if not any("paddle_inference" in path.name for path in libraries):
        raise RuntimeError("Paddle inference dynamic library was not found in the SDK")

    copied = []
    for source in [*libraries, bridge]:
        destination = output / source.name
        shutil.copy2(source, destination)
        copied.append(
            {
                "name": destination.name,
                "sha256": digest(destination),
                "size": destination.stat().st_size,
            }
        )

    version_file = sdk / "version.txt"
    paddle_version = None
    if version_file.is_file():
        shutil.copy2(version_file, output / "version.txt")
        paddle_version = version_file.read_text(encoding="utf-8").strip()

    manifest = {
        "schemaVersion": 1,
        "runtime": "paddle-inference",
        "bridgeAbiVersion": BRIDGE_ABI_VERSION,
        "paddleVersion": paddle_version,
        "platform": sys.platform,
        "architecture": os.environ.get("PROCESSOR_ARCHITECTURE"),
        "libraries": sorted(copied, key=lambda item: item["name"].casefold()),
    }
    (output / "runtime-manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"packaged Paddle runtime: {output}")
    print(f"dynamic libraries: {len(copied)}")


def parse_args() -> argparse.Namespace:
    repository = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True, help="extracted Paddle SDK root")
    parser.add_argument("--output", type=Path, required=True, help="runtime output directory")
    parser.add_argument("--build-dir", type=Path, help="persistent CMake build directory")
    parser.add_argument("--cmake", default=shutil.which("cmake") or "cmake")
    parser.add_argument("--generator", help="optional CMake generator")
    parser.add_argument("--config", default="Release", help="CMake configuration")
    parser.add_argument("--force", action="store_true", help="overwrite output files")
    parser.set_defaults(repository=repository)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    sdk = args.sdk.resolve()
    output = args.output.resolve()
    require_sdk(sdk)
    prepare_output(output, args.force)

    if args.build_dir:
        build = args.build_dir.resolve()
        build.mkdir(parents=True, exist_ok=True)
        bridge = build_bridge(
            args.repository,
            sdk,
            build,
            args.cmake,
            args.config,
            args.generator,
        )
        package(sdk, bridge, output)
        return

    with tempfile.TemporaryDirectory(prefix="latexsnipper-paddle-build-") as directory:
        bridge = build_bridge(
            args.repository,
            sdk,
            Path(directory),
            args.cmake,
            args.config,
            args.generator,
        )
        package(sdk, bridge, output)


if __name__ == "__main__":
    main()
