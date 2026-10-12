"""Formal PyO3 registry smoke, usable against an installed/extracted wheel."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import threading
import time
from latexsnipper_core import FormulaRegistry, LaTeXSnipperError


def expect(code: str, operation) -> None:
    try:
        operation()
    except LaTeXSnipperError as error:
        assert error.code == code, (code, error.code)
        assert error.retryable is False
        assert hasattr(error, "details")
    else:
        raise AssertionError(f"expected {code}")


def smoke() -> None:
    arguments = dict(input_format="latex", output_format="omml", context_sha256="a" * 64)
    with FormulaRegistry() as first, FormulaRegistry() as second:
        assert first.closed is False
        assert first.info()["abiVersion"] == 1 and "handle" not in first.info()
        assert first.capabilities()["routes"]
        assert first.approvals() == {"pending": [], "grants": []}
        report = first.convert(r"\frac{1}{2}", **arguments)
        assert report["outcome"]["kind"] == "converted"
        assert "<m:f>" in report["outcome"]["result"]["candidate"]["content"]
        alternate = first.convert("y^3", backend="missing:backend", fallback=["core:builtin"], **arguments)
        assert alternate["outcome"]["kind"] == "converted" and len(alternate["attempts"]) == 2
        assert alternate["decisionSha256"] != report["decisionSha256"]
        rejected = first.convert(r"\unknownmacro+x", **arguments)
        assert rejected["outcome"]["kind"] == "rejected" and len(rejected["attempts"]) == 1
        expect("LIMIT_EXCEEDED", lambda: first.convert("x" * (64 * 1024 + 1), **arguments))
        expect("UNKNOWN_BACKEND", lambda: first.set_enabled("missing:backend", True))
        expect("DUPLICATE_REGISTRATION", lambda: first.convert("x", fallback=["core:builtin", "core:builtin"], **arguments))
        first.cancel()
        expect("FORMULA_REGISTRY_CANCELLED", lambda: first.convert("x", **arguments))
        assert second.convert("x^2", **arguments)["outcome"]["kind"] == "converted"
        first.close(); first.close()
        assert first.closed is True
        expect("FORMULA_REGISTRY_NOT_FOUND", first.capabilities)
    assert first.closed and second.closed
    holder = None
    try:
        with FormulaRegistry() as holder:
            raise RuntimeError("caller failure")
    except RuntimeError:
        pass
    assert holder is not None and holder.closed
    with FormulaRegistry() as feature_probe:
        wasi_compiled = feature_probe.info()["wasiFormulaCompiled"]
    if not wasi_compiled:
        expect("FEATURE_NOT_COMPILED", lambda: FormulaRegistry(plugin_store="absent-store", profile_json="{}"))
    expect("INVALID_ARGUMENT", lambda: FormulaRegistry(plugin_store="absent-store"))
    print("Python registry: formal class, audited reports, fallback, refusal, independent lifecycle and cleanup passed")


def profile(exporter: bool) -> dict:
    return {"schemaVersion": 1, "pluginId": "fixture.component", "backendId": "fixture:profile",
            "transport": "application/vnd.fixture.power",
            "operation": {"kind": "exporter", "input": "latex"} if exporter else {"kind": "importer", "output": "omml"},
            "format": {"id": "fixture:power", "mimeType": "application/vnd.fixture.power",
                       "description": "Finite power fixture", "validation": {"kind": "regex", "pattern": "[a-z]:[0-9]"}}}


def wasi_smoke(root: Path) -> None:
    expect("INVALID_JSON", lambda: FormulaRegistry(plugin_store=root,
           profile_json='{"schemaVersion":1,"schemaVersion":2}'))
    for exporter in (False, True):
        with FormulaRegistry(plugin_store=root, profile_json=json.dumps(profile(exporter))) as registry:
            assert registry.info()["wasiFormulaCompiled"] is True
            arguments = dict(input_format="latex" if exporter else "fixture:power",
                             output_format="fixture:power" if exporter else "omml",
                             context_sha256="a" * 64, mode="best-effort", backend="fixture:profile")
            source = "$$z^7$$" if exporter else "x:2"
            assert registry.convert(source, **arguments)["outcome"]["error"]["code"] == "BACKEND_DISABLED"
            registry.set_enabled("fixture:profile", True)
            for source in ("$$z^7$$", "x^2") if exporter else ("x:2", "y:4"):
                report = registry.convert(source, **arguments)
                assert report["outcome"]["kind"] == "converted", report
                content = report["outcome"]["result"]["candidate"]["content"]
                if exporter:
                    assert content == ("x:2" if source == "x^2" else "z:7")
                else:
                    assert "m:sSup" in content
            expect("PLUGIN_WASI_RESOURCE_POLICY", lambda: registry.approval('{"kind":"once","requestId":"unknown"}'))
            assert registry.approvals()["grants"] == []
    for exporter, linear_format, source, expected in (
        (False, "unicode-math", "y:4", "〖y〗^(4)"),
        (True, "ascii-math", "{:x:}^(2)", "x:2"),
    ):
        definition = profile(exporter)
        definition["operation"] = {"kind": "exporter", "input": linear_format} if exporter else {
            "kind": "importer", "output": linear_format}
        with FormulaRegistry(plugin_store=root, profile_json=json.dumps(definition)) as registry:
            arguments = dict(input_format=linear_format if exporter else "fixture:power",
                             output_format="fixture:power" if exporter else linear_format,
                             context_sha256="a" * 64, mode="best-effort", backend="fixture:profile")
            assert registry.convert(source, **arguments)["outcome"]["error"]["code"] == "BACKEND_DISABLED"
            registry.set_enabled("fixture:profile", True)
            report = registry.convert(source, **arguments)
            assert report["outcome"]["kind"] == "converted", report
            assert report["outcome"]["result"]["candidate"]["content"] == expected
            assert registry.approvals()["grants"] == []
    loop_profile = profile(False)
    loop_profile["format"]["validation"]["pattern"] = "[a-z]:[0-9]|control:infinite"
    with FormulaRegistry(plugin_store=root, profile_json=json.dumps(loop_profile)) as registry:
        registry.set_enabled("fixture:profile", True)
        results = []
        started = threading.Event()
        def run_loop() -> None:
            started.set()
            retry_deadline = time.monotonic() + 5
            while time.monotonic() < retry_deadline:
                try:
                    results.append(registry.convert("control:infinite", input_format="fixture:power",
                        output_format="omml", context_sha256="a" * 64, mode="best-effort", backend="fixture:profile"))
                    return
                except LaTeXSnipperError as error:
                    # Observation also acquires the instance mutex. Retry only
                    # pre-execution BUSY, never an executed report or failure.
                    if error.code == "FORMULA_REGISTRY_BUSY":
                        time.sleep(0.001)
                        continue
                    results.append(error.code)
                    return
        producer = threading.Thread(target=run_loop, daemon=True)
        producer.start()
        assert started.wait(timeout=5)
        time.sleep(0.001)
        busy_seen = False
        deadline = time.monotonic() + 5
        while producer.is_alive() and time.monotonic() < deadline:
            try:
                registry.capabilities()
            except LaTeXSnipperError as error:
                assert error.code == "FORMULA_REGISTRY_BUSY", error.code
                busy_seen = True
                registry.cancel()
                break
            time.sleep(0.001)
        producer.join(timeout=5)
        assert not producer.is_alive(), "Native invocation did not finish after cancellation"
        assert busy_seen, f"No active conversion observed; cancellation remains unproven: {results}"
        assert results == ["FORMULA_REGISTRY_CANCELLED"], results
    print("Python registry WASI: real import/export, raw profile validation, explicit route enable and default-deny consent passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture-store", type=Path)
    args = parser.parse_args()
    smoke()
    if args.fixture_store is not None:
        wasi_smoke(args.fixture_store)
