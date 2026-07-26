#!/usr/bin/env python3
"""Measure file-read and OS mmap mechanics without claiming ORT session gains."""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import mmap
import os
import platform
import statistics
import subprocess
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path


def elapsed_ms(operation):
    started = time.perf_counter_ns()
    value = operation()
    return (time.perf_counter_ns() - started) / 1_000_000, value


def percentile(values: list[float], percentile: float) -> float:
    ordered = sorted(values)
    index = round((len(ordered) - 1) * percentile)
    return ordered[index]


def summarize(values: list[float]) -> dict[str, float]:
    return {
        "min": min(values),
        "median": statistics.median(values),
        "p95": percentile(values, 0.95),
        "max": max(values),
    }


def memory_snapshot() -> dict[str, int | None]:
    if os.name == "nt":
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            _fields_ = [
                ("cb", wintypes.DWORD),
                ("PageFaultCount", wintypes.DWORD),
                ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t),
            ]

        counters = Counters()
        counters.cb = ctypes.sizeof(counters)
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetCurrentProcess.restype = wintypes.HANDLE
        process = ctypes.WinDLL("psapi", use_last_error=True)
        process.GetProcessMemoryInfo.argtypes = [
            wintypes.HANDLE,
            ctypes.POINTER(Counters),
            wintypes.DWORD,
        ]
        process.GetProcessMemoryInfo.restype = wintypes.BOOL
        if process.GetProcessMemoryInfo(
            kernel.GetCurrentProcess(), ctypes.byref(counters), counters.cb
        ):
            return {
                "workingSetBytes": int(counters.WorkingSetSize),
                "peakWorkingSetBytes": int(counters.PeakWorkingSetSize),
            }
        return {"workingSetBytes": None, "peakWorkingSetBytes": None}
    try:
        import resource

        peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        peak_bytes = int(peak if platform.system() == "Darwin" else peak * 1024)
        return {"workingSetBytes": None, "peakWorkingSetBytes": peak_bytes}
    except (ImportError, OSError):
        return {"workingSetBytes": None, "peakWorkingSetBytes": None}


def mmap_once(path: Path) -> tuple[float, float, int]:
    with path.open("rb") as source:
        open_ms, mapping = elapsed_ms(
            lambda: mmap.mmap(source.fileno(), 0, access=mmap.ACCESS_READ)
        )
        try:
            def touch_pages() -> int:
                checksum = 0
                page_size = mmap.PAGESIZE
                for offset in range(0, len(mapping), page_size):
                    checksum ^= mapping[offset]
                if mapping:
                    checksum ^= mapping[-1]
                return checksum

            touch_ms, checksum = elapsed_ms(touch_pages)
            return open_ms, touch_ms, checksum
        finally:
            mapping.close()


def windows_lock_probe() -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="latexsnipper-mmap-probe-") as temp:
        root = Path(temp)
        current = root / "current.bin"
        replacement = root / "replacement.bin"
        current.write_bytes(b"current")
        replacement.write_bytes(b"replacement")
        with current.open("rb") as source:
            mapping = mmap.mmap(source.fileno(), 0, access=mmap.ACCESS_READ)
            try:
                try:
                    os.replace(replacement, current)
                    while_open = "succeeded"
                except OSError as error:
                    while_open = f"failed:{error.winerror if os.name == 'nt' else error.errno}"
            finally:
                mapping.close()
        if not replacement.exists():
            replacement.write_bytes(b"replacement-closed")
        try:
            os.replace(replacement, current)
            after_close = "succeeded"
        except OSError as error:
            after_close = f"failed:{error.winerror if os.name == 'nt' else error.errno}"
        return {"replaceWhileMapped": while_open, "replaceAfterClose": after_close}


def git_commit(root: Path) -> str | None:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("model", type=Path)
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    path = args.model.resolve()
    if not path.is_file():
        parser.error(f"model is not a file: {path}")
    if args.iterations < 1:
        parser.error("--iterations must be positive")

    read_times = []
    mmap_open_times = []
    mmap_touch_times = []
    memory_before = memory_snapshot()
    for _ in range(args.iterations):
        read_ms, payload = elapsed_ms(path.read_bytes)
        read_times.append(read_ms)
        del payload
        open_ms, touch_ms, _ = mmap_once(path)
        mmap_open_times.append(open_ms)
        mmap_touch_times.append(touch_ms)
    memory_after = memory_snapshot()

    repo = Path(__file__).resolve().parents[2]
    try:
        model_label = path.relative_to(repo).as_posix()
    except ValueError:
        model_label = path.name
    report = {
        "schemaVersion": 1,
        "scope": "io_only_not_ort_session",
        "timestampUtc": datetime.now(timezone.utc).isoformat(),
        "coreCommit": git_commit(repo),
        "model": {
            "path": model_label,
            "sizeBytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        },
        "environment": {
            "os": platform.platform(),
            "python": platform.python_version(),
            "pageSizeBytes": mmap.PAGESIZE,
        },
        "iterations": args.iterations,
        "bufferedReadMs": summarize(read_times),
        "mmapOpenMs": summarize(mmap_open_times),
        "mmapPageTouchMs": summarize(mmap_touch_times),
        "memory": {"before": memory_before, "after": memory_after},
        "fileReplacementProbe": windows_lock_probe(),
        "limitations": [
            "Measures OS file I/O mechanics only.",
            "Does not create an ONNX Runtime session.",
            "Warm filesystem cache can affect every timing.",
        ],
    }
    encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    print(encoded, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
