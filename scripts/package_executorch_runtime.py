#!/usr/bin/env python3
"""Build and package the LaTeXSnipper ExecuTorch C bridge.

The input is an installed ExecuTorch C++ SDK built with the required backend
(P2 uses Windows x64 + XNNPACK). The output is a redistributable runtime
directory; end users do not need Python, PyTorch, CMake, or ExecuTorch headers.
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
    "win32": "latexsnipper_executorch_bridge.dll",
    "linux": "liblatexsnipper_executorch_bridge.so",
    "darwin": "liblatexsnipper_executorch_bridge.dylib",
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def require_sdk(sdk: Path) -> None:
    header = sdk / "include" / "executorch" / "extension" / "module" / "module.h"
    config = sdk / "lib" / "cmake" / "ExecuTorch" / "executorch-config.cmake"
    if not header.is_file() or not config.is_file():
        raise RuntimeError(
            f"{sdk} is not an installed ExecuTorch SDK "
            "(expected extension/module headers and CMake package config)"
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
    runtime_version: str,
) -> Path:
    source = repository / "crates" / "runtime-executorch" / "native"
    install = build / "install"
    configure = [
        cmake,
        "-S",
        str(source),
        "-B",
        str(build),
        f"-DCMAKE_PREFIX_PATH={sdk}",
        f"-DCMAKE_INSTALL_PREFIX={install}",
        f"-DCMAKE_BUILD_TYPE={configuration}",
        f"-DLATEXSNIPPER_EXECUTORCH_VERSION={runtime_version}",
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


def package(bridge: Path, output: Path, runtime_version: str) -> None:
    destination = output / bridge.name
    shutil.copy2(bridge, destination)
    (output / "version.txt").write_text(runtime_version + "\n", encoding="utf-8")
    manifest = {
        "schemaVersion": 1,
        "runtime": "executorch",
        "bridgeAbiVersion": BRIDGE_ABI_VERSION,
        "executorchVersion": runtime_version,
        "platform": sys.platform,
        "architecture": os.environ.get("PROCESSOR_ARCHITECTURE"),
        "delegates": ["xnnpack", "cpu"],
        "libraries": [
            {
                "name": destination.name,
                "sha256": digest(destination),
                "size": destination.stat().st_size,
            }
        ],
    }
    (output / "runtime-manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"packaged ExecuTorch runtime: {output}")


def parse_args() -> argparse.Namespace:
    repository = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runtime-version", default="1.3.1")
    parser.add_argument("--build-dir", type=Path)
    parser.add_argument("--cmake", default=shutil.which("cmake") or "cmake")
    parser.add_argument("--generator")
    parser.add_argument("--config", default="Release")
    parser.add_argument("--force", action="store_true")
    parser.set_defaults(repository=repository)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    sdk = args.sdk.resolve()
    output = args.output.resolve()
    require_sdk(sdk)
    prepare_output(output, args.force)

    def run(build: Path) -> None:
        bridge = build_bridge(
            args.repository,
            sdk,
            build,
            args.cmake,
            args.config,
            args.generator,
            args.runtime_version,
        )
        package(bridge, output, args.runtime_version)

    if args.build_dir:
        build = args.build_dir.resolve()
        build.mkdir(parents=True, exist_ok=True)
        run(build)
    else:
        with tempfile.TemporaryDirectory(
            prefix="latexsnipper-executorch-build-"
        ) as directory:
            run(Path(directory))


if __name__ == "__main__":
    main()
