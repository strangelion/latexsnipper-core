"""Verify legacy RGB ABI compatibility against the actual native shared library."""
from __future__ import annotations

import argparse
import ctypes
import json
import sys
import tempfile
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor

from python_session import Library, Session


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--library-dir", required=True, type=Path)
    args = parser.parse_args()
    filename = {"win32": "latexsnipper_ffi.dll", "darwin": "liblatexsnipper_ffi.dylib"}.get(sys.platform, "liblatexsnipper_ffi.so")
    library = Library(args.library_dir / filename)
    dll = library._dll
    dll.latexsnipper_init.argtypes = [ctypes.c_char_p]
    dll.latexsnipper_init.restype = ctypes.c_int32
    dll.latexsnipper_ffi_response_version.argtypes = []
    dll.latexsnipper_ffi_response_version.restype = ctypes.c_uint32
    dll.latexsnipper_release.argtypes = []
    dll.latexsnipper_release.restype = None
    dll.latexsnipper_free_string.argtypes = [ctypes.c_void_p]
    dll.latexsnipper_free_string.restype = None
    functions = []
    for name in ("formula", "cropped_formula", "text", "mixed"):
        function = getattr(dll, "latexsnipper_recognize_" + name)
        function.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_uint32, ctypes.c_uint32]
        function.restype = ctypes.c_void_p
        functions.append(function)

    def read(pointer: int) -> dict:
        assert pointer
        try:
            value = json.loads(ctypes.string_at(pointer).decode("utf-8"))
        finally:
            dll.latexsnipper_free_string(pointer)
        assert value["versions"]["ffiResponseVersion"] == 3
        assert value["done"] is True
        return value

    pixels = ctypes.create_string_buffer(bytes([255, 255, 255, 0]))

    def recognize() -> dict:
        value = read(functions[1](pixels, 3, 1, 1))
        # No real models are installed in this fixture: never accept fake output.
        assert isinstance(value.get("error"), str) and value["error"]
        assert "not initialized" not in value["error"]
        assert "latex" not in value and "text" not in value
        return value

    assert dll.latexsnipper_ffi_response_version() == 3
    dll.latexsnipper_release()
    dll.latexsnipper_free_string(None)
    with tempfile.TemporaryDirectory(prefix="legacy-c-") as temporary:
        models = Path(temporary) / "models-unicode-\u4e2d\u6587"
        models.mkdir()
        with Session(library, models, max_threads=1) as independent:
            independent.warmup("croppedFormula")
            try:
                path = str(models).encode("utf-8")
                assert dll.latexsnipper_init(path) == 1
                for invalid in (None, b"", b" ", b"\xff", b"x" * (1024 * 1024 + 1), str(models / "missing").encode("utf-8")):
                    assert dll.latexsnipper_init(invalid) == 0
                    recognize()
                with ThreadPoolExecutor(max_workers=2) as pool:
                    list(pool.map(lambda _: recognize(), range(4)))
                for function in functions:
                    for pointer, length, width, height in ((None,3,1,1), (pixels,0,1,1), (pixels,2,1,1), (pixels,100*1024*1024+1,1,1), (pixels,3,0,1), (pixels,3,1,2**32-1), (pixels,100*1024*1024,10000,10000)):
                        value = read(function(pointer, length, width, height))
                        assert value.get("error") and "latex" not in value
                # Preserve the historical padding rule and failed-init isolation.
                assert read(functions[1](pixels,4,1,1)).get("error")
                assert dll.latexsnipper_init(path) == 1
                recognize()
                dll.latexsnipper_release()
                dll.latexsnipper_release()
                assert read(functions[0](pixels,3,1,1))["error"] == "Engine not initialized"
                assert independent.health()["runtime"]["initialized"] is True
                assert independent.warmup("croppedFormula")["alreadyWarm"] is True
            finally:
                dll.latexsnipper_release()
    print("Actual legacy C ABI: RGB bounds, version/free, lifecycle, Unicode, concurrency and generic-session isolation passed")


if __name__ == "__main__":
    main()
