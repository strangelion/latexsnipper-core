"""Installed-extension smoke test for persistent Python session semantics."""

from __future__ import annotations

import base64
from concurrent.futures import ThreadPoolExecutor
import tempfile
from pathlib import Path

from latexsnipper_core import (
    CancellationToken, LaTeXSnipperError, Session, __version__,
    convert_formula, formula_conversion_capabilities,
    complete_formula, apply_formula_completion, formula_completion_capabilities,
    complete_formula_scoped, apply_formula_completion_scoped, formula_scoped_completion_capabilities,
)


def formula_smoke() -> None:
    routes = formula_conversion_capabilities()
    assert len(routes) == 176
    assert len({(r["input"], r["output"], r["mode"]) for r in routes}) == 176
    samples = {
        "latex": r"\frac{a}{b}",
        "mathml": "<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>",
        "omml": '<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">'
                '<m:f><m:num><m:r><m:t>a</m:t></m:r></m:num>'
                '<m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>',
        "typst": "frac(a, b)",
        "markdown": r"$\frac{a}{b}$",
        "unicode-math": "a/b",
        "ascii-math": "a/b",
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
    assert executed == 78
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


def completion_smoke() -> None:
    caps = formula_completion_capabilities()
    assert caps["inputFormats"] == ["latex"]
    assert caps["offsetEncoding"] == "utf-8-bytes"
    assert caps["revisionEncoding"] == "canonical-decimal-u64-string"
    assert caps["revisionArgumentEncoding"] == "python-int-u64"
    assert caps["maxResultDataBytes"] == 1024 * 1024
    source = "中\\fr"
    cursor = len(source.encode("utf-8"))
    for revision in (0, 2**53 + 1, 2**64 - 1):
        result = complete_formula(source, cursor=cursor, revision=revision)
        assert result["snapshot"]["revision"] == str(revision)
        assert result["completion"]["context"]["revision"] == str(revision)
        assert any(item["id"] == "builtin:frac" for item in result["completion"]["candidates"])
        selected = apply_formula_completion(
            source, cursor=cursor, revision=revision, snapshot=result["snapshot"],
            candidate_id="builtin:frac",
        )
        assert selected == {
            "source": "中\\frac{}{}", "cursor": 9,
            "tabStops": [{"start": 9, "end": 9}, {"start": 11, "end": 11}],
        }
    valid = dict(source=source, cursor=cursor, revision=2**64 - 1)
    for override, code in [
        ({"revision": True}, "INVALID_ARGUMENT"),
        ({"revision": -1}, "INVALID_ARGUMENT"),
        ({"revision": 2**64}, "INVALID_ARGUMENT"),
        ({"revision": "1"}, "INVALID_ARGUMENT"),
        ({"revision": 1.0}, "INVALID_ARGUMENT"),
        ({"input_format": "typst"}, "INVALID_ARGUMENT"),
        ({"cursor": 1}, "INVALID_CURSOR"),
        ({"source": "x" * 65_537}, "LIMIT_EXCEEDED"),
        ({"source": "x" * 4096}, "LIMIT_EXCEEDED"),
        ({"source": "{" * 64 + "x"}, "LIMIT_EXCEEDED"),
    ]:
        try:
            complete_formula(**{**valid, **override})
        except LaTeXSnipperError as error:
            assert error.code == code, (code, error.code)
            assert error.retryable is False
        else:
            raise AssertionError(f"expected completion error {code}")
    result = complete_formula(**valid)
    apply = {**valid, "snapshot": result["snapshot"], "candidate_id": "builtin:frac"}
    for override, code in [
        ({"source": "中\\sr"}, "STALE_CONTEXT"),
        ({"cursor": cursor - 1}, "STALE_CONTEXT"),
        ({"revision": 0}, "STALE_CONTEXT"),
        ({"candidate_id": "external:untrusted"}, "UNKNOWN_CANDIDATE"),
        ({"snapshot": {**result["snapshot"], "insert": "untrusted"}}, "INVALID_ARGUMENT"),
        ({"snapshot": {**result["snapshot"], "revision": 1}}, "INVALID_ARGUMENT"),
        ({"snapshot": {**result["snapshot"], "revision": "01"}}, "INVALID_ARGUMENT"),
        ({"snapshot": {**result["snapshot"], "sourceSha256": "x" * 4096}}, "INVALID_ARGUMENT"),
    ]:
        try:
            apply_formula_completion(**{**apply, **override})
        except LaTeXSnipperError as error:
            assert error.code == code, (code, error.code)
        else:
            raise AssertionError(f"expected acceptance error {code}")
    for parameters in ({**valid, "insert": "untrusted"}, {**apply, "replace": {}}):
        try:
            if "snapshot" in parameters:
                apply_formula_completion(**parameters)
            else:
                complete_formula(**parameters)
        except TypeError:
            pass
        else:
            raise AssertionError("unknown keyword argument succeeded")
    for literal in (r"\text{\fr", r"%\fr", r"\verb|\fr"):
        assert not complete_formula(literal, cursor=len(literal), revision=0)["completion"]["candidates"]
    matrix = r"\begin{matrix}a&\fr"
    matrix_result = complete_formula(matrix, cursor=len(matrix), revision=1)
    assert matrix_result["completion"]["context"]["cell"] == {"environment": "matrix", "row": 0, "column": 1}

    def isolated(index: int) -> None:
        item = complete_formula(r"x^", cursor=2, revision=index)
        assert item["snapshot"]["revision"] == str(index)
        assert apply_formula_completion(
            r"x^", cursor=2, revision=index, snapshot=item["snapshot"],
            candidate_id="slot:power-two",
        )["source"] == "x^2"

    with ThreadPoolExecutor(max_workers=4) as executor:
        list(executor.map(isolated, range(64)))


def scoped_completion_smoke() -> None:
    from copy import deepcopy

    caps = formula_scoped_completion_capabilities()
    assert caps["schemaVersion"] == 2 and caps["maxDefinitions"] == 64
    assert caps["maxCatalogBytes"] == 65536
    assert caps["revisionArgumentEncoding"] == "python-int-u64"
    assert not caps["macroExpansion"] and not caps["persistence"]
    catalog = {
        "scope": {"userSha256": "a" * 64, "projectSha256": "b" * 64,
                  "documentSha256": "c" * 64, "generation": str(2**64 - 1)},
        "definitions": [{"id": "pair", "revision": str(2**64 - 1), "userSha256": "a" * 64,
                         "projectSha256": "b" * 64, "command": "custompair", "arguments": ["math", "text"]}],
    }
    params = dict(source="中\\cust", cursor=8, revision=2**64 - 1, catalog=catalog)
    reply = complete_formula_scoped(**params)
    assert reply["snapshot"]["revision"] == str(2**64 - 1)
    assert reply["candidates"][0]["definition"] == {"id": "pair", "revision": str(2**64 - 1)}
    apply = {**params, "snapshot": reply["snapshot"], "candidate_id": "definition:pair"}
    assert apply_formula_completion_scoped(**apply) == {
        "source": "中\\custompair{}{}", "cursor": 15,
        "tabStops": [{"start": 15, "end": 15}, {"start": 17, "end": 17}],
    }
    for field in ("generation", "projectSha256", "documentSha256", "signature", "definitionRevision"):
        changed = deepcopy(catalog)
        if field == "signature":
            changed["definitions"][0]["arguments"] = []
        elif field == "definitionRevision":
            changed["definitions"][0]["revision"] = "0"
        else:
            changed["scope"][field] = "0" if field == "generation" else "d" * 64
        try:
            apply_formula_completion_scoped(**{**apply, "catalog": changed})
        except LaTeXSnipperError as error:
            assert error.code == "STALE_CONTEXT"
        else:
            raise AssertionError("stale scoped edit succeeded")
    for override, code in [({"revision": value}, "INVALID_ARGUMENT") for value in (True, -1, 2**64, "1", 1.0)] + [
        ({"cursor": 1}, "INVALID_CURSOR"), ({"source": "x" * 65537}, "LIMIT_EXCEEDED"),
        ({"input_format": "typst"}, "INVALID_ARGUMENT"),
        ({"catalog": {**catalog, "body": "untrusted"}}, "INVALID_ARGUMENT"),
        ({"catalog": {**catalog, "extra": "x" * 65536}}, "INVALID_ARGUMENT"),
    ]:
        try:
            complete_formula_scoped(**{**params, **override})
        except LaTeXSnipperError as error:
            assert error.code == code and not error.retryable
        else:
            raise AssertionError(f"invalid scoped query succeeded: {code}")
    for override, code in [
        ({"candidate_id": "unknown"}, "UNKNOWN_CANDIDATE"),
        ({"snapshot": {**reply["snapshot"], "revision": 0}}, "INVALID_ARGUMENT"),
        ({"snapshot": {**reply["snapshot"], "insert": "untrusted"}}, "INVALID_ARGUMENT"),
        ({"snapshot": {**reply["snapshot"], "definitionsSha256": "x" * 4096}}, "INVALID_ARGUMENT"),
    ]:
        try:
            apply_formula_completion_scoped(**{**apply, **override})
        except LaTeXSnipperError as error:
            assert error.code == code
        else:
            raise AssertionError(f"invalid scoped acceptance succeeded: {code}")
    foreign = deepcopy(catalog)
    foreign["scope"]["userSha256"] = "d" * 64
    hidden = complete_formula_scoped(**{**params, "catalog": foreign})
    assert hidden["candidates"] == [] and "custompair" not in str(hidden)
    literal = r"\custompair{x}{\fr"
    assert complete_formula_scoped(literal, cursor=len(literal), revision=0, catalog=catalog)["context"]["inText"]
    for function, arguments in [(complete_formula_scoped, {**params, "insert": "untrusted"}),
                                (apply_formula_completion_scoped, {**apply, "replace": {}})]:
        try:
            function(**arguments)
        except TypeError:
            pass
        else:
            raise AssertionError("unknown scoped keyword succeeded")

    def isolated(index: int) -> None:
        local = deepcopy(catalog)
        local["scope"]["generation"] = str(index)
        queried = complete_formula_scoped(**{**params, "catalog": local, "revision": index})
        selected = apply_formula_completion_scoped(**{**apply, "catalog": local,
            "revision": index, "snapshot": queried["snapshot"]})
        assert selected["source"] == "中\\custompair{}{}"
    with ThreadPoolExecutor(max_workers=4) as executor:
        list(executor.map(isolated, range(64)))


def main() -> None:
    formula_smoke()
    completion_smoke()
    scoped_completion_smoke()
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
        assert complete_formula(r"\fr", cursor=3, revision=0)["completion"]["candidates"]
        assert first.warmup("formula")["alreadyWarm"] is True

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
