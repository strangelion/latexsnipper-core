"""Exercise private fragment APIs from the actual installed or extracted wheel."""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
import json
import latexsnipper_core as package

from latexsnipper_core import (
    LaTeXSnipperError, complete_formula_fragments, apply_formula_completion_fragments,
    formula_fragment_completion_capabilities, formula_completion_capabilities,
    formula_scoped_completion_capabilities,
)


def expect_error(function, arguments, code: str) -> None:
    try:
        function(**arguments)
    except LaTeXSnipperError as error:
        assert error.code == code, (code, error.code)
        assert error.retryable is False
    else:
        raise AssertionError(f"expected fragment error {code}")


def fragment_completion_smoke() -> None:
    for name in ("complete_formula_fragments", "apply_formula_completion_fragments", "formula_fragment_completion_capabilities"):
        assert name in package.__all__ and callable(getattr(package, name))
    caps = formula_fragment_completion_capabilities()
    assert caps["schemaVersion"] == 3 and caps["maxFragments"] == 64
    assert caps["maxCatalogBytes"] == 1024 * 1024 and caps["maxSnapshotBytes"] == 4096
    assert caps["maxResultDataBytes"] == 1024 * 1024 and caps["maxFragmentBytes"] == 8192
    assert caps["revisionArgumentEncoding"] == "python-int-u64"
    assert not caps["persistence"] and not caps["learning"] and not caps["macroExpansion"]
    assert formula_completion_capabilities()["schemaVersion"] == 1
    assert formula_scoped_completion_capabilities()["schemaVersion"] == 2
    assert formula_scoped_completion_capabilities()["maxCatalogBytes"] == 65536
    revision = 2**64 - 1
    catalog = {"commands": {"scope": {"userSha256": "a" * 64, "projectSha256": "b" * 64,
            "documentSha256": "c" * 64, "generation": str(revision)}, "definitions": []},
        "policy": {"retrievalEnabled": True, "sensitiveDocument": False, "allowInSensitiveDocument": False},
        "fragments": [{"id": "saved", "revision": str(revision), "userSha256": "a" * 64,
            "projectSha256": "b" * 64, "documentSha256": "c" * 64, "trigger": "bayes",
            "label": "Saved formula", "source": r"\frac{α}{}"}]}
    params = dict(source="中😀\\bay", cursor=11, revision=revision, catalog=catalog)
    result = complete_formula_fragments(**params)
    assert result["snapshot"]["revision"] == str(revision)
    assert result["candidates"][0]["definition"] is None
    assert result["candidates"][0]["fragment"] == {"id": "saved", "revision": str(revision)}
    apply = {**params, "snapshot": result["snapshot"], "candidate_id": "fragment:saved"}
    assert apply_formula_completion_fragments(**apply) == {
        "source": "中😀\\frac{α}{}", "cursor": 17, "tabStops": [{"start": 17, "end": 17}]}
    for field in ("generation", "userSha256", "projectSha256", "documentSha256", "source", "fragmentRevision",
                  "retrievalEnabled", "sensitiveDocument", "allowInSensitiveDocument", "clear"):
        changed = deepcopy(catalog)
        if field == "source":
            changed["fragments"][0]["source"] = "x+1"
        elif field == "fragmentRevision":
            changed["fragments"][0]["revision"] = "0"
        elif field == "clear":
            changed["fragments"] = []
        elif field in changed["policy"]:
            changed["policy"][field] = field != "retrievalEnabled"
        else:
            changed["commands"]["scope"][field] = "0" if field == "generation" else "d" * 64
        expect_error(apply_formula_completion_fragments, {**apply, "catalog": changed}, "STALE_CONTEXT")
    for override, code in [({"revision": True}, "INVALID_ARGUMENT"), ({"revision": "1"}, "INVALID_ARGUMENT"),
        ({"revision": 1.5}, "INVALID_ARGUMENT"), ({"revision": -1}, "INVALID_ARGUMENT"),
        ({"revision": 2**64}, "INVALID_ARGUMENT"), ({"input_format": "typst"}, "INVALID_ARGUMENT"),
        ({"cursor": 1}, "INVALID_CURSOR"), ({"source": "x" * 65537}, "LIMIT_EXCEEDED")]:
        expect_error(complete_formula_fragments, {**params, **override}, code)
    expect_error(apply_formula_completion_fragments, {**apply, "revision": 0}, "STALE_CONTEXT")
    expect_error(apply_formula_completion_fragments, {**apply, "candidate_id": "unknown"}, "UNKNOWN_CANDIDATE")
    for override in ({"schemaVersion": 2}, {"catalogSha256": "b" * 64}):
        expect_error(apply_formula_completion_fragments,
            {**apply, "snapshot": {**result["snapshot"], **override}}, "STALE_CONTEXT")
    for override in ({"revision": 0}, {"revision": "01"}, {"insert": "untrusted"}, {"sourceSha256": "a" * 4097}):
        expect_error(apply_formula_completion_fragments,
            {**apply, "snapshot": {**result["snapshot"], **override}}, "INVALID_ARGUMENT")
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
        hidden = complete_formula_fragments(**{**params, "catalog": changed})
        assert hidden["candidates"] == [] and "PRIVATE_SECRET" not in str(hidden)
        base = complete_formula_fragments(**{**params, "catalog": changed, "source": r"\fr", "cursor": 3})
        assert any(item["edit"]["id"] == "builtin:frac" for item in base["candidates"])
    for field in ("fragmentRevision", "generation", "policy", "extra", "catalog"):
        changed = deepcopy(catalog)
        if field == "fragmentRevision":
            changed["fragments"][0]["revision"] = 42
        elif field == "generation":
            changed["commands"]["scope"]["generation"] = "01"
        elif field == "policy":
            changed["policy"]["retrievalEnabled"] = "true"
        elif field == "extra":
            changed["fragments"][0]["filePath"] = "not-opened"
        else:
            changed["extra"] = True
        expect_error(complete_formula_fragments, {**params, "catalog": changed}, "INVALID_ARGUMENT")
    for source, code in [(r"\frac{", "INVALID_CATALOG"), (r"$$x$$", "INVALID_CATALOG"),
        ("x % trailing", "INVALID_CATALOG"), ("x" * 8193, "LIMIT_EXCEEDED")]:
        changed = deepcopy(catalog)
        changed["fragments"][0]["source"] = source
        expect_error(complete_formula_fragments, {**params, "catalog": changed}, code)
    too_large = deepcopy(catalog)
    too_large["fragments"][0]["source"] = "x" * (1024 * 1024)
    expect_error(complete_formula_fragments, {**params, "catalog": too_large}, "INVALID_ARGUMENT")
    invalid_unicode = deepcopy(catalog)
    invalid_unicode["fragments"][0]["source"] = "\ud800"
    expect_error(complete_formula_fragments, {**params, "catalog": invalid_unicode}, "INVALID_ARGUMENT")
    large = deepcopy(catalog)
    item = large["fragments"][0]
    item["source"] = "x %" + '"' * 8188 + "\n"
    large["fragments"] = [{**item, "id": f"entry-{index}"} for index in range(60)]
    assert len(json.dumps(large).encode("utf-8")) <= 1024 * 1024
    large_source = "x" * 3900 + r"\bay"
    expect_error(complete_formula_fragments, {**params, "source": large_source, "cursor": len(large_source),
        "catalog": large}, "OUTPUT_TOO_LARGE")
    unicode_catalog = deepcopy(catalog)
    unicode_entry = unicode_catalog["fragments"][0]
    unicode_entry["source"] = "α" * 3900
    unicode_catalog["fragments"] = [{**unicode_entry, "id": f"entry-{index}"} for index in range(64)]
    assert len(json.dumps(unicode_catalog, ensure_ascii=False).encode("utf-8")) <= caps["maxCatalogBytes"]
    assert len(json.dumps(unicode_catalog).encode("utf-8")) > caps["maxCatalogBytes"]
    unicode_result = complete_formula_fragments(**{**params, "catalog": unicode_catalog,
        "source": r"\nomatch", "cursor": 8})
    assert unicode_result["candidates"] == []
    for function, arguments in [(complete_formula_fragments, {**params, "insert": "untrusted"}),
        (apply_formula_completion_fragments, {**apply, "replace": {}})]:
        try:
            function(**arguments)
        except TypeError:
            pass
        else:
            raise AssertionError("unknown fragment keyword succeeded")

    def isolated(index: int) -> None:
        local = deepcopy(catalog)
        local["commands"]["scope"]["generation"] = str(index)
        local["fragments"][0]["source"] = str(index) + r"+\frac{}{}"
        queried = complete_formula_fragments(**{**params, "catalog": local, "revision": index})
        selected = apply_formula_completion_fragments(**{**apply, "catalog": local,
            "revision": index, "snapshot": queried["snapshot"]})
        assert selected["source"] == "中😀" + local["fragments"][0]["source"]
    with ThreadPoolExecutor(max_workers=4) as executor:
        list(executor.map(isolated, range(64)))
    assert complete_formula_fragments(**params)["snapshot"]["schemaVersion"] == 3
    print("Python fragments: real exports, policy guards, bounded refusal and isolated concurrent edits passed")


if __name__ == "__main__":
    fragment_completion_smoke()
