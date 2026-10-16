"""Load a built wheel in isolated subprocesses without changing any environment."""
from __future__ import annotations

import argparse
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tempfile
import zipfile


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("wheel", type=Path)
    parser.add_argument("--expect-wasi", action="store_true")
    parser.add_argument("--expect-protected-preferences", action="store_true")
    parser.add_argument("--fixture-store", type=Path)
    args = parser.parse_args()
    tests = Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(prefix="snipper-python-wheel-") as directory:
        root = Path(directory)
        with zipfile.ZipFile(args.wheel) as archive:
            entries = archive.infolist()
            if len(entries) > 2048 or sum(entry.file_size for entry in entries) > 512 * 1024 * 1024:
                raise ValueError("Wheel exceeds the extraction budget")
            for entry in entries:
                path = PurePosixPath(entry.filename)
                if path.is_absolute() or ".." in path.parts or "\\" in entry.filename or ":" in entry.filename:
                    raise ValueError("Unsafe wheel path")
                if (entry.external_attr >> 16) & 0o170000 == 0o120000:
                    raise ValueError("Wheel symlinks are not supported")
            archive.extractall(root)
        env = os.environ.copy()
        env["PYTHONPATH"] = str(root)
        env["SNIPPER_EXPECTED_WHEEL_ROOT"] = str(root)
        env["SNIPPER_EXPECTED_WASI"] = "1" if args.expect_wasi else "0"
        env["SNIPPER_EXPECTED_PROTECTED_PREFERENCES"] = "1" if args.expect_protected_preferences else "0"
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        check = """import os
from pathlib import Path
import latexsnipper_core._native as native
from latexsnipper_core import FormulaRegistry, FormulaPreferences
assert Path(native.__file__).resolve().is_relative_to(Path(os.environ['SNIPPER_EXPECTED_WHEEL_ROOT']).resolve())
with FormulaRegistry() as registry:
    assert registry.info()['wasiFormulaCompiled'] == (os.environ['SNIPPER_EXPECTED_WASI'] == '1')
assert FormulaPreferences.storage_capabilities()['available'] == (os.environ['SNIPPER_EXPECTED_PROTECTED_PREFERENCES'] == '1')
print('Verified wheel-native origin and explicit feature identity')
"""
        subprocess.run([sys.executable, "-c", check], env=env, cwd=root, check=True, timeout=120)
        subprocess.run([sys.executable, str(tests / "smoke.py")], env=env, cwd=root, check=True, timeout=180)
        command = [sys.executable, str(tests / "registry_smoke.py")]
        if args.fixture_store is not None:
            command += ["--fixture-store", str(args.fixture_store.resolve())]
        subprocess.run(command, env=env, cwd=root, check=True, timeout=180)
        subprocess.run([sys.executable, str(tests / "preference_storage_smoke.py")], env=env, cwd=root, check=True, timeout=180)
    print("Extracted wheel smoke passed; isolated native files removed")


if __name__ == "__main__":
    main()
