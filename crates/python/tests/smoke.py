"""Installed-extension smoke test for persistent Python session semantics."""

from __future__ import annotations

import base64
import tempfile
from pathlib import Path

from latexsnipper_core import (
    CancellationToken, LaTeXSnipperError, Session, __version__,
    convert_formula, formula_conversion_capabilities,
)


def formula_smoke() -> None:
    routes = formula_conversion_capabilities()
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
    executed = 0
    for route in routes:
        assert route["target"] == "native"
        assert route["limitations"]
        assert route["available"] == (route["unavailableReason"] is None)
        arguments = dict(input_format=route["input"], output_format=route["output"], mode=route["mode"])
        if route["available"]:
            result = convert_formula(samples[route["input"]], **arguments)
            assert isinstance(result, str) and result
            executed += 1
        else:
            try:
                convert_formula("x", **arguments)
            except LaTeXSnipperError as error:
                assert error.code == "UNSUPPORTED_FORMAT"
                assert error.detail == route["unavailableReason"]
                assert error.retryable is False
            else:
                raise AssertionError(f"unavailable route succeeded: {route}")
    assert executed == 46
    for input_format in ("latex", "typst", "mathml", "omml"):
        bare = convert_formula(
            samples[input_format], input_format=input_format,
            output_format="latex-fragment", mode="best-effort",
        )
        assert bare == r"\frac{a}{b}"
        inline = convert_formula(
            samples[input_format], input_format=input_format,
            output_format="markdown_inline", mode="best-effort",
        )
        assert inline == r"$\frac{a}{b}$"
    assert r"\documentclass" in convert_formula(
        samples["typst"], input_format="typst", output_format="latex", mode="best-effort",
    )
    assert "<m:f>" in convert_formula(samples["latex"], input_format="latex", output_format="omml")
    failures = [
        ("x", {"input_format": "ole"}, "INVALID_ARGUMENT"),
        ("x", {"output_format": "pdf"}, "INVALID_ARGUMENT"),
        ("x", {"mode": "lossless"}, "INVALID_ARGUMENT"),
        (" ", {}, "CONVERSION_FAILED"),
        (r"\unknownmacro+x", {}, "CONVERSION_FAILED"),
        ("x" * (64 * 1024 + 1), {}, "INPUT_TOO_LARGE"),
        ("x+" * 6000 + "x", {}, "OUTPUT_TOO_LARGE"),
        ("frac(a,b)", {"input_format": "typst", "output_format": "latex-fragment"}, "UNSUPPORTED_FORMAT"),
        (r"\documentclass{article}x", {"output_format": "latex-fragment", "mode": "best-effort"}, "CONVERSION_FAILED"),
        ("{" * 65 + "x" + "}" * 65, {"mode": "best-effort"}, "INPUT_TOO_LARGE"),
        (r"\sqrt" * 513, {"mode": "best-effort"}, "INPUT_TOO_LARGE"),
    ]
    for source, override, code in failures:
        arguments = {"input_format": "latex", "output_format": "omml", **override}
        try:
            convert_formula(source, **arguments)
        except LaTeXSnipperError as error:
            assert error.code == code, (code, error.code, error.detail)
            assert error.retryable is False
        else:
            raise AssertionError(f"expected {code}")
    # Conversion failures must not affect later calls or the recognition adapter.
    assert "<m:f>" in convert_formula(samples["latex"], input_format=" LaTeX ", output_format="OMML")


def main() -> None:
    formula_smoke()
    with tempfile.TemporaryDirectory() as first_root, tempfile.TemporaryDirectory() as second_root:
        first = Session(Path(first_root))
        second = Session(Path(second_root), runtime_preference="cpu")

        assert __version__
        assert first.closed is False
        assert second.closed is False
        assert isinstance(first.health(), dict)
        assert isinstance(first.capabilities(), dict)

        first_warmup = first.warmup("formula")
        second_warmup = first.warmup("formula")
        assert first_warmup["alreadyWarm"] is False
        assert second_warmup["alreadyWarm"] is True

        try:
            first.recognize_bytes(memoryview(b"not-an-image"), format_hint="png")
        except LaTeXSnipperError as error:
            assert error.code in {"INVALID_INPUT", "IMAGE_DECODE_FAILED"}
            assert isinstance(error.retryable, bool)
        else:
            raise AssertionError("invalid encoded image unexpectedly succeeded")

        # A request failure must not poison this session or leak into another.
        assert isinstance(first.health(), dict)
        assert isinstance(second.health(), dict)

        cancellation = CancellationToken()
        assert cancellation.cancelled is False
        cancellation.cancel()
        assert cancellation.cancelled is True
        try:
            first.recognize_bytes(
                memoryview(b"not-an-image"),
                cancellation=cancellation,
            )
        except LaTeXSnipperError as error:
            assert error.code == "CANCELLED"
        else:
            raise AssertionError("cancelled request unexpectedly succeeded")

        one_pixel_png = base64.b64decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwC"
            "AAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        )
        try:
            first.recognize_bytes(memoryview(one_pixel_png), timeout_ms=0)
        except LaTeXSnipperError as error:
            assert error.code == "TIMEOUT"
        else:
            raise AssertionError("zero-timeout request unexpectedly succeeded")

        assert isinstance(first.health(), dict)

        first.close()
        first.close()
        assert first.closed is True
        try:
            first.health()
        except LaTeXSnipperError as error:
            assert error.code == "INVALID_INPUT"
            assert str(error) == "The recognition session is closed."
        else:
            raise AssertionError("closed session unexpectedly accepted a request")

        second.close()

    with tempfile.TemporaryDirectory() as model_root:
        with Session(model_root) as managed:
            assert managed.closed is False
        assert managed.closed is True


if __name__ == "__main__":
    main()
