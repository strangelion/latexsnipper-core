"""Compile public C/C++ fragment consumers; remove only owned temporary products."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--library-dir", required=True, type=Path)
    args = parser.parse_args()
    library_dir = args.library_dir.resolve(strict=True)
    ffi_root = Path(__file__).resolve().parents[1]
    source = ffi_root / "tests" / "c" / "formula_fragment_smoke.c"
    include = ffi_root / "include"
    windows = os.name == "nt"
    library = library_dir / ("latexsnipper_ffi.dll" if windows else
        "liblatexsnipper_ffi.dylib" if sys.platform == "darwin" else "liblatexsnipper_ffi.so")
    if not library.is_file():
        parser.error("Build the native shared library before running this check")
    env = dict(os.environ)
    env["PATH"] = str(library_dir) + os.pathsep + env.get("PATH", "")
    env.setdefault("VSLANG", "1033")
    if windows:
        compiler = shutil.which("cl")
        if compiler is None:
            parser.error("Run inside an x64 Visual Studio developer environment with cl available")
    else:
        compilers = [shutil.which("cc"), shutil.which("c++")]
        if not all(compilers):
            parser.error("Both cc and c++ are required for the public header check")
    with tempfile.TemporaryDirectory(prefix="latexsnipper-c-fragment-") as directory:
        temporary = Path(directory)
        for index, language in enumerate(("c", "c++")):
            executable = temporary / (f"fragment-{index}.exe" if windows else f"fragment-{index}")
            if windows:
                command = [compiler, "/nologo", "/W4", "/WX", "/utf-8", "/D_CRT_SECURE_NO_WARNINGS",
                    "/TC" if language == "c" else "/TP", "/std:c11" if language == "c" else "/std:c++17",
                    "/I" + str(include), str(source), "/Fo" + str(temporary / f"fragment-{index}.obj"),
                    "/Fe" + str(executable), "/link", str(library_dir / "latexsnipper_ffi.dll.lib")]
            else:
                command = [compilers[index], "-x", language, "-std=c11" if language == "c" else "-std=c++17",
                    "-Wall", "-Wextra", "-Werror", "-I", str(include), str(source), "-L", str(library_dir),
                    "-llatexsnipper_ffi", "-Wl,-rpath," + str(library_dir), "-o", str(executable)]
            subprocess.run(command, cwd=temporary, env=env, check=True)
            subprocess.run([str(executable)], cwd=temporary, env=env, check=True)
    assert not temporary.exists(), "Owned compiler products were not removed"
    print("Public C/C++ header smoke passed; owned temporary compiler products removed")


if __name__ == "__main__":
    main()
