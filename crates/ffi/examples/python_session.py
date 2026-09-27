"""Zero-dependency Python client for the persistent LaTeXSnipper C session ABI.

This reference client is intentionally small. Production Python wheels should
wrap the same lifecycle with PyO3, but they must preserve these handle, error,
and close semantics.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import struct
import tempfile
import zlib
from pathlib import Path
from typing import Any


class CoreError(RuntimeError):
    """Stable error returned by the Core v3 JSON envelope."""

    def __init__(
        self,
        code: str,
        message: str,
        recoverable: bool,
        details: Any | None = None,
    ) -> None:
        super().__init__(f"{code}: {message}")
        self.code = code
        self.recoverable = recoverable
        self.details = details


class SessionClosedError(RuntimeError):
    """Raised before calling Core when a Python session is already closed."""


def _one_pixel_png() -> bytes:
    def chunk(kind: bytes, payload: bytes) -> bytes:
        checksum = zlib.crc32(kind + payload) & 0xFFFFFFFF
        return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", checksum)

    signature = b"\x89PNG\r\n\x1a\n"
    header = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
    pixels = zlib.compress(b"\x00\xff\xff\xff")
    return signature + chunk(b"IHDR", header) + chunk(b"IDAT", pixels) + chunk(b"IEND", b"")


class Library:
    """Loaded Core shared library and its typed C ABI declarations."""

    def __init__(self, path: str | Path) -> None:
        self.path = Path(path).resolve(strict=True)
        self._dll = ctypes.CDLL(str(self.path))

        self._dll.latexsnipper_session_abi_version.argtypes = []
        self._dll.latexsnipper_session_abi_version.restype = ctypes.c_uint32

        self._dll.latexsnipper_session_create.argtypes = [
            ctypes.c_void_p,
            ctypes.c_size_t,
        ]
        self._dll.latexsnipper_session_create.restype = ctypes.c_void_p

        self._dll.latexsnipper_session_health.argtypes = [ctypes.c_uint64]
        self._dll.latexsnipper_session_health.restype = ctypes.c_void_p

        self._dll.latexsnipper_session_warmup.argtypes = [
            ctypes.c_uint64,
            ctypes.c_void_p,
            ctypes.c_size_t,
        ]
        self._dll.latexsnipper_session_warmup.restype = ctypes.c_void_p

        self._dll.latexsnipper_session_recognize_bytes.argtypes = [
            ctypes.c_uint64,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.c_size_t,
        ]
        self._dll.latexsnipper_session_recognize_bytes.restype = ctypes.c_void_p

        self._dll.latexsnipper_session_reload_models.argtypes = [ctypes.c_uint64]
        self._dll.latexsnipper_session_reload_models.restype = ctypes.c_void_p

        self._dll.latexsnipper_session_close.argtypes = [ctypes.c_uint64]
        self._dll.latexsnipper_session_close.restype = ctypes.c_void_p

        self._dll.latexsnipper_string_free.argtypes = [ctypes.c_void_p]
        self._dll.latexsnipper_string_free.restype = None

        version = int(self._dll.latexsnipper_session_abi_version())
        if version != 1:
            raise RuntimeError(f"unsupported application-session ABI version: {version}")

    @staticmethod
    def _encode(payload: dict[str, Any]) -> bytes:
        return json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode(
            "utf-8"
        )

    def _decode(self, pointer: int | None) -> dict[str, Any]:
        if not pointer:
            raise MemoryError("Core could not allocate a JSON response")
        try:
            raw = ctypes.string_at(pointer).decode("utf-8")
        finally:
            self._dll.latexsnipper_string_free(pointer)
        envelope = json.loads(raw)
        if not envelope.get("ok", False):
            error = envelope.get("error") or {}
            raise CoreError(
                str(error.get("code", "INTERNAL")),
                str(error.get("message", "Core operation failed")),
                bool(error.get("recoverable", False)),
                error.get("details"),
            )
        return envelope["data"]

    def create(self, request: dict[str, Any]) -> int:
        request_bytes = self._encode(request)
        request_buffer = ctypes.create_string_buffer(request_bytes)
        data = self._decode(
            self._dll.latexsnipper_session_create(
                ctypes.cast(request_buffer, ctypes.c_void_p),
                len(request_bytes),
            )
        )
        return int(data["handle"])

    def health(self, handle: int) -> dict[str, Any]:
        return self._decode(self._dll.latexsnipper_session_health(handle))

    def warmup(self, handle: int, profile: str) -> dict[str, Any]:
        request_bytes = self._encode({"profile": profile})
        request_buffer = ctypes.create_string_buffer(request_bytes)
        return self._decode(
            self._dll.latexsnipper_session_warmup(
                handle,
                ctypes.cast(request_buffer, ctypes.c_void_p),
                len(request_bytes),
            )
        )

    def recognize_bytes(
        self,
        handle: int,
        data: bytes,
        *,
        profile: str,
        parse_mode: str | None = None,
        timeout_ms: int | None = None,
        strict: bool = False,
    ) -> dict[str, Any]:
        request: dict[str, Any] = {"profile": profile, "strict": strict}
        if parse_mode is not None:
            request["parseMode"] = parse_mode
        if timeout_ms is not None:
            request["timeoutMs"] = timeout_ms
        request_bytes = self._encode(request)
        request_buffer = ctypes.create_string_buffer(request_bytes)
        buffer = ctypes.create_string_buffer(data)
        return self._decode(
            self._dll.latexsnipper_session_recognize_bytes(
                handle,
                ctypes.cast(request_buffer, ctypes.c_void_p),
                len(request_bytes),
                ctypes.cast(buffer, ctypes.c_void_p),
                len(data),
            )
        )

    def reload_models(self, handle: int) -> dict[str, Any]:
        return self._decode(self._dll.latexsnipper_session_reload_models(handle))

    def close(self, handle: int) -> dict[str, Any]:
        return self._decode(self._dll.latexsnipper_session_close(handle))


class Session:
    """A persistent, independently closable Core recognition session."""

    def __init__(
        self,
        library: Library,
        models_dir: str | Path,
        *,
        runtime_preference: str = "auto",
        parse_mode: str = "specialized_stable",
        max_threads: int = 4,
        quality_baselines_dir: str | Path | None = None,
        provider_smoke_fixture: str | Path | None = None,
    ) -> None:
        self._library = library
        request: dict[str, Any] = {
            "modelsDir": str(Path(models_dir)),
            "runtimePreference": runtime_preference,
            "parseMode": parse_mode,
            "maxThreads": max_threads,
        }
        if quality_baselines_dir is not None:
            request["qualityBaselinesDir"] = str(Path(quality_baselines_dir))
        if provider_smoke_fixture is not None:
            request["providerSmokeFixture"] = str(Path(provider_smoke_fixture))
        self._handle: int | None = library.create(request)

    @property
    def closed(self) -> bool:
        return self._handle is None

    def _require_handle(self) -> int:
        if self._handle is None:
            raise SessionClosedError("the Python session is already closed")
        return self._handle

    def health(self) -> dict[str, Any]:
        return self._library.health(self._require_handle())

    def warmup(self, profile: str) -> dict[str, Any]:
        return self._library.warmup(self._require_handle(), profile)

    def recognize_bytes(self, data: bytes, **options: Any) -> dict[str, Any]:
        return self._library.recognize_bytes(
            self._require_handle(), data, **options
        )

    def reload_models(self) -> dict[str, Any]:
        return self._library.reload_models(self._require_handle())

    def close(self) -> None:
        handle = self._handle
        if handle is None:
            return
        try:
            self._library.close(handle)
        finally:
            self._handle = None

    def __enter__(self) -> Session:
        self._require_handle()
        return self

    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> None:
        self.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--library", required=True, type=Path)
    parser.add_argument("--models", required=True, type=Path)
    args = parser.parse_args()

    library = Library(args.library)
    with tempfile.TemporaryDirectory(prefix="latexsnipper-python-session-") as other_models:
        with (
            Session(library, args.models, max_threads=1) as session,
            Session(library, other_models, max_threads=1) as independent_session,
        ):
            health = session.health()
            first = session.warmup("formula")
            second = session.warmup("formula")
            independent = independent_session.warmup("formula")
            recognition_error: str | None = None
            try:
                session.recognize_bytes(_one_pixel_png(), profile="croppedFormula")
            except CoreError as error:
                recognition_error = error.code
            health_after_recognition = session.health()
            print(
                json.dumps(
                    {
                        "runtimeInitialized": health["runtime"]["initialized"],
                        "firstAlreadyWarm": first["alreadyWarm"],
                        "secondAlreadyWarm": second["alreadyWarm"],
                        "independentSessionAlreadyWarm": independent["alreadyWarm"],
                        "recognitionError": recognition_error,
                        "healthyAfterRecognition": health_after_recognition["runtime"][
                            "initialized"
                        ],
                    },
                    ensure_ascii=False,
                )
            )


if __name__ == "__main__":
    main()
