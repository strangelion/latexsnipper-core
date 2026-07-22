#!/usr/bin/env python3
"""Build and package a LaTeXSnipper TensorRT or TensorRT-RTX native bridge."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


BRIDGE_ABI_VERSION = 1
BRIDGE_NAMES = {
    "tensorrt": {
        "win32": "latexsnipper_tensorrt_bridge.dll",
        "linux": "liblatexsnipper_tensorrt_bridge.so",
    },
    "tensorrt-rtx": {
        "win32": "latexsnipper_tensorrt_rtx_bridge.dll",
        "linux": "liblatexsnipper_tensorrt_rtx_bridge.so",
    },
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def prepare_output(output: Path, force: bool) -> None:
    if output.exists() and any(output.iterdir()) and not force:
        raise RuntimeError(
            f"output directory is not empty: {output}; pass --force to overwrite files"
        )
    output.mkdir(parents=True, exist_ok=True)


def require_sdk(root: Path, runtime: str) -> str:
    version_header = root / "include" / "NvInferVersion.h"
    if not version_header.is_file():
        raise RuntimeError(f"{root} is not a TensorRT SDK (NvInferVersion.h missing)")
    numeric_defines: dict[str, str] = {}
    pattern = re.compile(r"^#define\s+([A-Z0-9_]+)\s+(\d+)(?:\s|$)")
    for line in version_header.read_text(encoding="utf-8").splitlines():
        match = pattern.match(line)
        if match:
            numeric_defines[match.group(1)] = match.group(2)
    fields = ("MAJOR", "MINOR", "PATCH", "BUILD")
    candidates = (
        (lambda field: f"NV_TENSORRT_{field}", lambda field: f"TRT_{field}_ENTERPRISE")
        if runtime == "tensorrt"
        else (lambda field: f"TRT_{field}_RTX",)
    )
    values: dict[str, str] = {}
    for field in fields:
        for candidate in candidates:
            if name := numeric_defines.get(candidate(field)):
                values[field] = name
                break
    if set(values) != set(fields):
        raise RuntimeError(
            f"could not parse {runtime} version from {version_header}; found {values}"
        )
    expected_major = "10" if runtime == "tensorrt" else "1"
    if values["MAJOR"] != expected_major:
        raise RuntimeError(
            f"{runtime} bridge expects major {expected_major}, got {values['MAJOR']}"
        )
    if runtime == "tensorrt-rtx" and int(values["MINOR"]) < 5:
        raise RuntimeError("TensorRT-RTX bridge requires SDK 1.5 or newer")
    return ".".join(values[name] for name in ("MAJOR", "MINOR", "PATCH", "BUILD"))


def build_bridge(
    repository: Path,
    tensorrt_root: Path,
    cuda_root: Path,
    build: Path,
    cmake: str,
    configuration: str,
    generator: str | None,
    runtime: str,
) -> Path:
    source = repository / "crates" / "runtime-tensorrt" / "native"
    install = build / "install"
    configure = [
        cmake,
        "-S",
        str(source),
        "-B",
        str(build),
        f"-DCMAKE_INSTALL_PREFIX={install}",
        f"-DCMAKE_BUILD_TYPE={configuration}",
        f"-DLATEXSNIPPER_TENSORRT_ROOT={tensorrt_root}",
        f"-DCUDAToolkit_ROOT={cuda_root}",
        f"-DLATEXSNIPPER_TENSORRT_RTX={'ON' if runtime == 'tensorrt-rtx' else 'OFF'}",
    ]
    if generator:
        configure.extend(["-G", generator])
    subprocess.run(configure, check=True)
    subprocess.run([cmake, "--build", str(build), "--config", configuration], check=True)
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
    name = BRIDGE_NAMES[runtime].get(sys.platform)
    if name is None:
        raise RuntimeError(f"unsupported packaging platform: {sys.platform}")
    matches = list(install.rglob(name))
    if len(matches) != 1:
        raise RuntimeError(f"expected one installed {name}, found {len(matches)}")
    return matches[0]


def dependency_patterns(runtime: str) -> tuple[list[str], list[str]]:
    if sys.platform == "win32":
        if runtime == "tensorrt-rtx":
            return (
                ["tensorrt_rtx_*.dll", "tensorrt_onnxparser_rtx_*.dll"],
                ["cudart64_*.dll"],
            )
        return (
            ["nvinfer_10.dll", "nvinfer_plugin_10.dll", "nvonnxparser_10.dll"],
            ["cudart64_*.dll"],
        )
    if sys.platform == "linux":
        if runtime == "tensorrt-rtx":
            return (
                ["libtensorrt_rtx.so*", "libtensorrt_onnxparser_rtx.so*"],
                ["libcudart.so.*"],
            )
        return (
            ["libnvinfer.so.10*", "libnvinfer_plugin.so.10*", "libnvonnxparser.so.10*"],
            ["libcudart.so.12*"],
        )
    raise RuntimeError(f"unsupported packaging platform: {sys.platform}")


def find_required(root: Path, patterns: list[str], kind: str) -> list[Path]:
    found: list[Path] = []
    for pattern in patterns:
        matches = [path for path in root.rglob(pattern) if path.is_file()]
        if not matches:
            raise RuntimeError(f"required {kind} library '{pattern}' not found under {root}")
        found.extend(matches)
    unique: dict[str, Path] = {}
    for path in found:
        unique[path.name] = path
    return list(unique.values())


def package(
    bridge: Path,
    output: Path,
    tensorrt_root: Path,
    cuda_root: Path,
    runtime_version: str,
    include_dependencies: bool,
    runtime: str,
) -> None:
    destination = output / bridge.name
    shutil.copy2(bridge, destination)
    packaged = [destination]
    if include_dependencies:
        tensorrt_patterns, cuda_patterns = dependency_patterns(runtime)
        dependencies = find_required(tensorrt_root, tensorrt_patterns, "TensorRT")
        dependencies.extend(find_required(cuda_root, cuda_patterns, "CUDA"))
        for dependency in dependencies:
            target = output / dependency.name
            shutil.copy2(dependency, target)
            packaged.append(target)

    (output / "version.txt").write_text(runtime_version + "\n", encoding="utf-8")
    manifest = {
        "schemaVersion": 1,
        "runtime": runtime,
        "bridgeAbiVersion": BRIDGE_ABI_VERSION,
        "tensorrtVersion": runtime_version,
        "platform": sys.platform,
        "architecture": os.environ.get("PROCESSOR_ARCHITECTURE"),
        "features": [
            "cuda",
            "onnx-parser",
            "dynamic-shapes",
            "engine-cache-v1",
            *(["aot-jit", "strongly-typed-model"] if runtime == "tensorrt-rtx" else []),
        ],
        "libraries": [
            {
                "name": path.name,
                "sha256": digest(path),
                "size": path.stat().st_size,
            }
            for path in packaged
        ],
    }
    (output / "runtime-manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"packaged {runtime} runtime: {output}")


def parse_args() -> argparse.Namespace:
    repository = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--runtime", choices=("tensorrt", "tensorrt-rtx"), default="tensorrt"
    )
    parser.add_argument("--tensorrt-root", type=Path, required=True)
    parser.add_argument("--cuda-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--build-dir", type=Path)
    parser.add_argument("--cmake", default=shutil.which("cmake") or "cmake")
    parser.add_argument("--generator")
    parser.add_argument("--config", default="Release")
    parser.add_argument("--without-runtime-dependencies", action="store_true")
    parser.add_argument("--force", action="store_true")
    parser.set_defaults(repository=repository)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    tensorrt_root = args.tensorrt_root.resolve()
    cuda_root = args.cuda_root.resolve()
    output = args.output.resolve()
    runtime_version = require_sdk(tensorrt_root, args.runtime)
    prepare_output(output, args.force)

    def run(build: Path) -> None:
        bridge = build_bridge(
            args.repository,
            tensorrt_root,
            cuda_root,
            build,
            args.cmake,
            args.config,
            args.generator,
            args.runtime,
        )
        package(
            bridge,
            output,
            tensorrt_root,
            cuda_root,
            runtime_version,
            not args.without_runtime_dependencies,
            args.runtime,
        )

    if args.build_dir:
        build = args.build_dir.resolve()
        build.mkdir(parents=True, exist_ok=True)
        run(build)
    else:
        with tempfile.TemporaryDirectory(prefix="latexsnipper-tensorrt-build-") as directory:
            run(Path(directory))


if __name__ == "__main__":
    main()
