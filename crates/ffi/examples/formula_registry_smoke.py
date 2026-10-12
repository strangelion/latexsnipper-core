"""Verify the additive registry ABI through an actual shared library."""
from __future__ import annotations

import argparse
import ctypes
import json
import sys
from pathlib import Path

from python_session import CoreError, Library


class FormulaRegistry:
    """Trusted host client; reports are candidates, never document write permits."""

    def __init__(self, library: Library, declaration: dict) -> None:
        self.library = library
        self.handle: int | None = None
        dll = library._dll
        for name in ("capabilities", "cancel", "close", "approvals"):
            function = getattr(dll, "latexsnipper_formula_registry_" + name)
            function.argtypes = [ctypes.c_uint64]
            function.restype = ctypes.c_void_p
        for name in ("convert", "set_enabled", "approval"):
            function = getattr(dll, "latexsnipper_formula_registry_" + name)
            function.argtypes = [ctypes.c_uint64, ctypes.c_void_p, ctypes.c_size_t]
            function.restype = ctypes.c_void_p
        dll.latexsnipper_formula_registry_create.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        dll.latexsnipper_formula_registry_create.restype = ctypes.c_void_p
        data = json.dumps(declaration, ensure_ascii=False).encode("utf-8")
        buffer = ctypes.create_string_buffer(data)
        created = library._decode(dll.latexsnipper_formula_registry_create(buffer, len(data)))
        self.handle = created["handle"]
        self.created = created

    def call(self, name: str, request: dict | None = None) -> dict:
        if self.handle is None:
            raise RuntimeError("Formula registry is closed")
        function = getattr(self.library._dll, "latexsnipper_formula_registry_" + name)
        if request is None:
            return self.library._decode(function(self.handle))
        data = json.dumps(request, ensure_ascii=False).encode("utf-8")
        buffer = ctypes.create_string_buffer(data)
        return self.library._decode(function(self.handle, buffer, len(data)))

    def close(self) -> None:
        if self.handle is not None:
            self.call("close")
            self.handle = None

    def __enter__(self) -> FormulaRegistry:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--library-dir", required=True, type=Path)
    args = parser.parse_args()
    filename = {"win32": "latexsnipper_ffi.dll", "darwin": "liblatexsnipper_ffi.dylib"}.get(
        sys.platform, "liblatexsnipper_ffi.so"
    )
    library = Library(args.library_dir / filename)
    with FormulaRegistry(library, {"schemaVersion": 1}) as registry:
        assert registry.call("capabilities")["routes"]
        assert registry.call("approvals") == {"pending": [], "grants": []}
        request = {"schemaVersion": 1, "input": "latex", "output": "omml", "mode": "strict",
                   "backend": None, "content": r"\frac{1}{2}", "contextSha256": "a" * 64}
        first = registry.call("convert", request)
        assert first["outcome"]["kind"] == "converted"
        assert "<m:f>" in first["outcome"]["result"]["candidate"]["content"]
        request["backend"] = "missing:backend"
        request["policy"] = {"schemaVersion": 1, "backends": ["core:builtin"]}
        report = registry.call("convert", request)
        assert report["outcome"]["kind"] == "converted" and len(report["attempts"]) == 2
        request["content"] = r"\unknownmacro+x"
        rejected = registry.call("convert", request)
        assert rejected["outcome"]["kind"] == "rejected"
        assert len(rejected["attempts"]) == 2
        assert rejected["attempts"][-1]["status"]["kind"] == "rejected"
        registry.call("cancel")
        try:
            registry.call("convert", request)
        except CoreError as error:
            assert error.code == "FORMULA_REGISTRY_CANCELLED"
        else:
            raise AssertionError("Cancelled registry produced a report")
    assert registry.handle is None
    print("C registry shared-library smoke: route, report, fallback, refusal, cancel and close passed")


if __name__ == "__main__":
    main()
