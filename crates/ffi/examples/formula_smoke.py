"""Exercise exported formula symbols through the built shared library."""

from __future__ import annotations

import argparse
import ctypes
import sys
import tempfile
from pathlib import Path

from python_session import CoreError, Library, Session


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--library-dir", required=True, type=Path)
    args = parser.parse_args()
    filename = {
        "win32": "latexsnipper_ffi.dll",
        "darwin": "liblatexsnipper_ffi.dylib",
    }.get(sys.platform, "liblatexsnipper_ffi.so")
    library = Library(args.library_dir / filename)
    routes = library.formula_capabilities()
    assert len(routes) == 144
    assert len({(r["input"], r["output"], r["mode"]) for r in routes}) == 144
    samples = {
        "latex": r"\frac{a}{b}",
        "mathml": "<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>",
        "omml": '<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">'
                '<m:f><m:num><m:r><m:t>a</m:t></m:r></m:num>'
                '<m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>',
        "typst": "frac(a, b)",
        "markdown": r"A fraction $\frac{a}{b}$",
    }
    successful = 0
    rejected = 0
    for route in routes:
        assert route["target"] == "native" and route["limitations"]
        arguments = dict(input_format=route["input"], output_format=route["output"], mode=route["mode"])
        try:
            result = library.convert_formula(samples.get(route["input"], "x"), **arguments)
        except CoreError as error:
            assert route["available"] is False
            assert error.code == "UNSUPPORTED_FORMAT"
            assert error.details["detail"] == route["unavailableReason"]
            assert error.recoverable is False
            rejected += 1
        else:
            assert route["available"] is True
            assert isinstance(result["content"], str) and result["content"]
            assert result["capability"] == route
            successful += 1
    assert (successful, rejected) == (46, 98)
    unicode_formula = r"\text{中文}+\frac{a}{b}"
    assert unicode_formula in library.convert_formula(
        unicode_formula, input_format="latex", output_format="latex", mode="best-effort",
    )["content"]

    failures = [
        (" ", {}, "CONVERSION_FAILED"),
        (r"\unknownmacro+x", {}, "CONVERSION_FAILED"),
        ("x", {"mode": "lossless"}, "INVALID_JSON"),
        ("x", {"input_format": "ole"}, "INVALID_JSON"),
        ("x", {"output_format": "pdf"}, "INVALID_ARGUMENT"),
        ("x" * (64 * 1024 + 1), {}, "INPUT_TOO_LARGE"),
        ("{" * 65 + "x" + "}" * 65, {"mode": "best-effort"}, "INPUT_TOO_LARGE"),
        ("<!DOCTYPE math><math><mi>x</mi></math>", {"input_format": "mathml", "mode": "best-effort"}, "CONVERSION_FAILED"),
    ]
    for content, override, code in failures:
        try:
            library.convert_formula(content, **{"input_format": "latex", "output_format": "omml", **override})
        except CoreError as error:
            assert error.code == code, (code, error.code, error.details)
        else:
            raise AssertionError(f"expected {code}")
    # Explicit lengths reject bad/null requests before dereferencing their buffers.
    function = library._dll.latexsnipper_formula_convert
    for pointer, length, code in [(None, 0, "INVALID_ARGUMENT"), (None, 1, "INVALID_ARGUMENT")]:
        try:
            library._decode(function(pointer, length))
        except CoreError as error:
            assert error.code == code
        else:
            raise AssertionError("null request succeeded")
    invalid = ctypes.create_string_buffer(b"\xff")
    try:
        library._decode(function(ctypes.cast(invalid, ctypes.c_void_p), 1))
    except CoreError as error:
        assert error.code == "INVALID_JSON"
    else:
        raise AssertionError("invalid UTF-8 succeeded")

    # New symbols must not alter persistent sessions or ownership/close behavior.
    with tempfile.TemporaryDirectory() as models:
        with Session(library, models, max_threads=1) as session:
            assert isinstance(session.health(), dict)
            assert session.warmup("formula")["alreadyWarm"] is False
            assert session.warmup("formula")["alreadyWarm"] is True
            assert "<m:f>" in library.convert_formula(samples["latex"], input_format="latex", output_format="omml")["content"]
            assert isinstance(session.health(), dict)
        assert session.closed
    print(f"C formula smoke: {successful} converted, {rejected} explicitly rejected; errors and sessions passed")


if __name__ == "__main__":
    main()
