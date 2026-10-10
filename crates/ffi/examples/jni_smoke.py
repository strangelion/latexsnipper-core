"""Exercise actual exported JNI methods with javac and a checked desktop JVM."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--library-dir", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    filename = {"win32": "latexsnipper_ffi.dll", "darwin": "liblatexsnipper_ffi.dylib"}.get(
        sys.platform, "liblatexsnipper_ffi.so"
    )
    library = (args.library_dir / filename).resolve(strict=True)
    java = shutil.which("java")
    javac = shutil.which("javac")
    if not java or not javac:
        raise SystemExit("A JDK with java and javac is required; this test cannot be skipped as passed")
    with tempfile.TemporaryDirectory(prefix="latexsnipper-jni-") as scratch:
        temporary = Path(scratch)
        classes = temporary / "classes"
        first = temporary / "models-\u4e2d\u6587"
        second = temporary / "models-second"
        first.mkdir()
        second.mkdir()
        subprocess.run(
            [javac, "-encoding", "UTF-8", "-d", str(classes),
             str(root / "crates/ffi/java/com/latexsnipper/core/NativeSessionBridge.java"),
             str(root / "crates/ffi/tests/java/BridgeSmoke.java")],
            check=True, timeout=60,
        )
        result = subprocess.run(
            [java, "-Xcheck:jni", "-Xmx256m", "-cp", str(classes), "BridgeSmoke",
             str(library), str(first), str(second)],
            check=False, timeout=120, capture_output=True, text=True, encoding="utf-8",
        )
        print(result.stdout, end="")
        if result.stderr:
            print(result.stderr, file=sys.stderr, end="")
        result.check_returncode()
        if "WARNING in native method" in result.stdout + result.stderr:
            raise SystemExit("The checked JVM reported an invalid JNI call")


if __name__ == "__main__":
    main()
