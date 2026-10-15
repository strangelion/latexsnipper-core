"""Exercise exported formula symbols through the built shared library."""

from __future__ import annotations

import argparse
import ctypes
from copy import deepcopy
import sys
import tempfile
from pathlib import Path

from python_session import CoreError, Library, Session


def fragment_completion_smoke(library: Library) -> None:
    caps = library.fragment_completion_capabilities()
    assert caps["schemaVersion"] == 3 and caps["maxFragments"] == 64
    assert caps["maxRequestBytes"] == 1024 * 1024 and caps["maxResultDataBytes"] == 1024 * 1024
    assert not caps["persistence"] and not caps["learning"] and not caps["macroExpansion"]
    revision = 2**64 - 1
    catalog = {
        "commands": {"scope": {"userSha256": "a" * 64, "projectSha256": "b" * 64,
            "documentSha256": "c" * 64, "generation": str(revision)}, "definitions": []},
        "policy": {"retrievalEnabled": True, "sensitiveDocument": False, "allowInSensitiveDocument": False},
        "fragments": [{"id": "saved", "revision": str(revision), "userSha256": "a" * 64,
            "projectSha256": "b" * 64, "documentSha256": "c" * 64, "trigger": "bayes",
            "label": "Saved formula", "source": r"\frac{α}{}"}],
    }
    source = "中😀\\bay"
    cursor = len(source.encode("utf-8"))
    params = dict(source=source, cursor=cursor, revision=revision, catalog=catalog)
    for _ in range(32):
        result = library.complete_formula_fragments(**params)
        assert result["snapshot"]["revision"] == str(revision)
        assert result["candidates"][0]["fragment"] == {"id": "saved", "revision": str(revision)}
        assert result["candidates"][0]["definition"] is None
        applied = library.apply_formula_completion_fragments(**params,
            snapshot=result["snapshot"], candidate_id="fragment:saved")
        assert applied == {"source": "中😀\\frac{α}{}", "cursor": 17, "tabStops": [{"start": 17, "end": 17}]}
    for field in ("generation", "userSha256", "projectSha256", "documentSha256", "body",
                  "retrievalEnabled", "sensitiveDocument", "allowInSensitiveDocument", "clear"):
        changed = deepcopy(catalog)
        if field == "body":
            changed["fragments"][0]["source"] = "x+1"
        elif field == "clear":
            changed["fragments"] = []
        elif field in changed["policy"]:
            changed["policy"][field] = field != "retrievalEnabled"
        else:
            changed["commands"]["scope"][field] = "0" if field == "generation" else "d" * 64
        try:
            library.apply_formula_completion_fragments(**{**params, "catalog": changed},
                snapshot=result["snapshot"], candidate_id="fragment:saved")
        except CoreError as error:
            assert error.code == "STALE_CONTEXT", (field, error.code)
        else:
            raise AssertionError("stale fragment acceptance succeeded")
    for field in ("userSha256", "projectSha256", "documentSha256", "disabled", "sensitive"):
        changed = deepcopy(catalog)
        changed["fragments"][0]["source"] = "x" * 8193
        changed["fragments"][0]["label"] = "PRIVATE_SECRET"
        if field == "disabled":
            changed["policy"]["retrievalEnabled"] = False
        elif field == "sensitive":
            changed["policy"]["sensitiveDocument"] = True
        else:
            changed["fragments"][0][field] = "d" * 64
        hidden = library.complete_formula_fragments(**{**params, "catalog": changed})
        assert hidden["candidates"] == [] and "PRIVATE_SECRET" not in str(hidden)
    large = deepcopy(catalog)
    item = large["fragments"][0]
    item["source"] = "x %" + '"' * 8188 + "\n"
    large["fragments"] = [{**item, "id": f"entry-{index}"} for index in range(60)]
    large_source = "x" * 3900 + r"\bay"
    try:
        library.complete_formula_fragments(**{**params, "source": large_source,
            "cursor": len(large_source), "catalog": large})
    except CoreError as error:
        assert error.code == "OUTPUT_TOO_LARGE", error.code
    else:
        raise AssertionError("oversized fragment result was not refused")
    assert library.complete_formula_fragments(**params)["snapshot"]["schemaVersion"] == 3
    print("C fragments: native exports, precise Unicode edits, scope/policy guards and result refusal passed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--library-dir", required=True, type=Path)
    args = parser.parse_args()
    filename = {
        "win32": "latexsnipper_ffi.dll",
        "darwin": "liblatexsnipper_ffi.dylib",
    }.get(sys.platform, "liblatexsnipper_ffi.so")
    library = Library(args.library_dir / filename)
    fragment_completion_smoke(library)
    routes = library.formula_capabilities()
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
    assert (successful, rejected) == (78, 98)
    for input_format in ("latex", "typst", "mathml", "omml"):
        bare = library.convert_formula(
            samples[input_format], input_format=input_format,
            output_format="latex-fragment", mode="best-effort",
        )
        assert bare["content"] == r"\frac{a}{b}"
        assert bare["contentKind"] == "latex-fragment"
        assert bare["capability"]["output"] == "latex_display"
        inline = library.convert_formula(
            samples[input_format], input_format=input_format,
            output_format="markdown_inline", mode="best-effort",
        )
        assert inline["content"] == r"$\frac{a}{b}$"
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
        ("x+" * 6000 + "x", {}, "OUTPUT_TOO_LARGE"),
        ("frac(a,b)", {"input_format": "typst", "output_format": "latex-fragment"}, "UNSUPPORTED_FORMAT"),
        (r"\documentclass{article}x", {"output_format": "latex-fragment", "mode": "best-effort"}, "CONVERSION_FAILED"),
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
    completion_caps = library.completion_capabilities()
    fence_source = "中\\left(x"
    sized_source = "中\\Bigl(x"
    sized_cursor = len(sized_source.encode("utf-8"))
    sized = library.complete_formula(sized_source, cursor=sized_cursor, revision=42)
    assert library.apply_formula_completion(sized_source, cursor=sized_cursor, revision=42,
        snapshot=sized["snapshot"], candidate_id="structure:close-delimiter")["source"] == "中\\Bigl(x\\Bigr)"
    starred = library.complete_formula(r"\oper", cursor=5, revision=42)
    assert library.apply_formula_completion(r"\oper", cursor=5, revision=42,
        snapshot=starred["snapshot"], candidate_id="builtin:operatorname-star")["source"] == r"\operatorname*{}"
    literal = r"\mbox{\fr"
    assert not library.complete_formula(literal, cursor=len(literal), revision=42)["completion"]["candidates"]
    fence_cursor = len(fence_source.encode("utf-8"))
    fence = library.complete_formula(fence_source, cursor=fence_cursor, revision=42)
    assert fence["completion"]["context"]["slot"] == "group"
    assert library.apply_formula_completion(
        fence_source, cursor=fence_cursor, revision=42, snapshot=fence["snapshot"],
        candidate_id="structure:close-delimiter",
    )["source"] == "中\\left(x\\right)"
    token_argument = r"\left(x^\ri"
    assert not any(item["id"] == "structure:close-delimiter-prefix" for item in
        library.complete_formula(token_argument, cursor=len(token_argument), revision=0)["completion"]["candidates"])
    assert completion_caps["inputFormats"] == ["latex"]
    assert completion_caps["offsetEncoding"] == "utf-8-bytes"
    assert completion_caps["maxResultDataBytes"] == 1024 * 1024
    source = "中\\fr"
    cursor = len(source.encode("utf-8"))
    revision = 2**64 - 1
    for _ in range(32):
        completion = library.complete_formula(source, cursor=cursor, revision=revision)
        assert completion["snapshot"]["revision"] == str(revision)
        assert completion["completion"]["context"]["revision"] == str(revision)
        applied = library.apply_formula_completion(
            source, cursor=cursor, revision=revision,
            snapshot=completion["snapshot"], candidate_id="builtin:frac",
        )
        assert applied == {
            "source": "中\\frac{}{}", "cursor": 9,
            "tabStops": [{"start": 9, "end": 9}, {"start": 11, "end": 11}],
        }
    for override, code in [
        ({"revision": 0}, "STALE_CONTEXT"),
        ({"source": "中\\sr"}, "STALE_CONTEXT"),
        ({"cursor": 1}, "INVALID_CURSOR"),
        ({"candidate_id": "external:untrusted"}, "UNKNOWN_CANDIDATE"),
    ]:
        try:
            library.apply_formula_completion(**{
                "source": source, "cursor": cursor, "revision": revision,
                "snapshot": completion["snapshot"], "candidate_id": "builtin:frac", **override,
            })
        except CoreError as error:
            assert error.code == code, (code, error.code)
        else:
            raise AssertionError(f"expected completion error {code}")
    scoped_caps = library.scoped_completion_capabilities()
    assert scoped_caps["schemaVersion"] == 2
    assert scoped_caps["maxDefinitions"] == 64
    assert scoped_caps["persistence"] is False
    assert scoped_caps["argumentKinds"] == ["math", "text", "optional-math", "optional-text"]
    assert scoped_caps["maxLeadingOptionalArguments"] == 1
    catalog = {
        "scope": {"userSha256": "a" * 64, "projectSha256": "b" * 64,
                  "documentSha256": "c" * 64, "generation": str(revision)},
        "definitions": [{"id": "pair", "revision": str(revision), "userSha256": "a" * 64,
                         "projectSha256": "b" * 64, "command": "custompair", "arguments": ["math", "text"]}],
    }
    custom_source = "中\\cust"
    custom_cursor = len(custom_source.encode("utf-8"))
    for kind in ("optional-math", "optional-text"):
        optional_catalog = deepcopy(catalog)
        optional_catalog["definitions"][0]["arguments"].insert(0, kind)
        optional = library.complete_formula_scoped(custom_source, cursor=custom_cursor, revision=revision, catalog=optional_catalog)
        optional_edit = library.apply_formula_completion_scoped(custom_source, cursor=custom_cursor, revision=revision,
            catalog=optional_catalog, snapshot=optional["snapshot"], candidate_id="definition:pair:optional")
        assert optional_edit["source"] == "中\\custompair[]{}{}" and len(optional_edit["tabStops"]) == 3
    for _ in range(32):
        scoped = library.complete_formula_scoped(custom_source, cursor=custom_cursor, revision=revision, catalog=catalog)
        assert scoped["context"]["schemaVersion"] == 2
        assert scoped["candidates"][0]["definition"] == {"id": "pair", "revision": str(revision)}
        selected = library.apply_formula_completion_scoped(
            custom_source, cursor=custom_cursor, revision=revision, catalog=catalog,
            snapshot=scoped["snapshot"], candidate_id="definition:pair",
        )
        assert selected == {"source": "中\\custompair{}{}", "cursor": 15,
                            "tabStops": [{"start": 15, "end": 15}, {"start": 17, "end": 17}]}
    for field in ("generation", "projectSha256", "documentSha256", "signature"):
        changed = deepcopy(catalog)
        if field == "signature":
            changed["definitions"][0]["arguments"] = []
        else:
            changed["scope"][field] = "0" if field == "generation" else "d" * 64
        try:
            library.apply_formula_completion_scoped(
                custom_source, cursor=custom_cursor, revision=revision, catalog=changed,
                snapshot=scoped["snapshot"], candidate_id="definition:pair",
            )
        except CoreError as error:
            assert error.code == "STALE_CONTEXT"
        else:
            raise AssertionError("stale scoped completion succeeded")
    foreign = deepcopy(catalog)
    foreign["scope"]["userSha256"] = "d" * 64
    hidden = library.complete_formula_scoped(custom_source, cursor=custom_cursor, revision=revision, catalog=foreign)
    assert hidden["candidates"] == [] and "custompair" not in str(hidden)
    invalid = deepcopy(catalog)
    invalid["definitions"][0]["command"] = "mysymbol2"
    try:
        library.complete_formula_scoped(custom_source, cursor=custom_cursor, revision=revision, catalog=invalid)
    except CoreError as error:
        assert error.code == "INVALID_CATALOG"
    else:
        raise AssertionError("non-control-word declaration succeeded")
    for symbol in ("latexsnipper_formula_complete", "latexsnipper_formula_completion_apply",
                   "latexsnipper_formula_complete_scoped", "latexsnipper_formula_completion_apply_scoped",
                   "latexsnipper_formula_complete_fragments", "latexsnipper_formula_completion_apply_fragments"):
        function = getattr(library._dll, symbol)
        function.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        function.restype = ctypes.c_void_p
        for length in (0, 1, 1024 * 1024 + 1):
            try:
                library._decode(function(None, length))
            except CoreError as error:
                assert error.code == "INVALID_ARGUMENT"
            else:
                raise AssertionError("null completion request succeeded")
        invalid = ctypes.create_string_buffer(b"\xff")
        try:
            library._decode(function(ctypes.cast(invalid, ctypes.c_void_p), 1))
        except CoreError as error:
            assert error.code == "INVALID_JSON"
        else:
            raise AssertionError("invalid completion UTF-8 succeeded")
    with tempfile.TemporaryDirectory() as models:
        with Session(library, models, max_threads=1) as session:
            assert isinstance(session.health(), dict)
            assert session.warmup("formula")["alreadyWarm"] is False
            assert session.warmup("formula")["alreadyWarm"] is True
            assert "<m:f>" in library.convert_formula(samples["latex"], input_format="latex", output_format="omml")["content"]
            assert library.complete_formula("\\fr", cursor=3, revision=0)["completion"]["candidates"]
            assert library.complete_formula_scoped(custom_source, cursor=custom_cursor, revision=revision, catalog=catalog)["candidates"]
            assert session.warmup("formula")["alreadyWarm"] is True
            assert isinstance(session.health(), dict)
        assert session.closed
    print(f"C formula smoke: {successful} converted, {rejected} explicitly rejected; completion, errors and sessions passed")


if __name__ == "__main__":
    main()
