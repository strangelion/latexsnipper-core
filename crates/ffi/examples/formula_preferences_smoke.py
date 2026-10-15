"""Validate the preference ABI against an actual shared library, not Rust linkage."""
import argparse
import ctypes
import json
from pathlib import Path
import secrets
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--library-dir", type=Path, required=True)
    args = parser.parse_args()
    filename = "latexsnipper_ffi.dll" if sys.platform == "win32" else "liblatexsnipper_ffi.dylib" if sys.platform == "darwin" else "liblatexsnipper_ffi.so"
    library = ctypes.CDLL(str(args.library_dir.resolve() / filename))
    free = library.latexsnipper_string_free
    free.argtypes = [ctypes.c_void_p]; free.restype = None
    functions = {}
    for name, signature in {
        "preferences_capabilities": [], "preferences_create": [ctypes.c_void_p, ctypes.c_size_t],
        "preferences_restore": [ctypes.c_void_p, ctypes.c_size_t],
        "preferences_command": [ctypes.c_uint64, ctypes.c_void_p, ctypes.c_size_t],
        "preferences_close": [ctypes.c_uint64], "complete_fragments": [ctypes.c_void_p, ctypes.c_size_t],
    }.items():
        function = getattr(library, "latexsnipper_formula_" + name)
        function.argtypes = signature; function.restype = ctypes.c_void_p; functions[name] = function

    def call(name, *prefix, body=None):
        if body is not None:
            encoded = body if isinstance(body, bytes) else json.dumps(body, ensure_ascii=False).encode("utf-8")
            buffer = ctypes.create_string_buffer(encoded)
            pointer = functions[name](*prefix, ctypes.cast(buffer, ctypes.c_void_p), len(encoded))
        else:
            pointer = functions[name](*prefix)
        assert pointer
        try:
            raw = ctypes.string_at(pointer)
            assert len(raw) <= 1024 * 1024 + 65536
            result = json.loads(raw)
        finally:
            free(pointer)
        if not result["ok"]:
            assert not result["error"].get("details")
        return result

    scope = dict(userSha256="a" * 64, projectSha256=None, documentSha256="c" * 64, generation="0")
    owners = []
    keys = {}
    def new():
        key = secrets.token_hex(32)
        result = call("preferences_create", body=dict(schemaVersion=1, scope=scope, keyHex=key))
        assert result["ok"], result
        handle = int(result["data"]["handle"]); owners.append(handle); keys[handle] = key; return handle
    def command(owner, kind, **fields):
        return call("preferences_command", owner, body=dict(kind=kind, **fields))
    policy = dict(rankingEnabled=True, feedbackEnabled=True, allowSensitiveFeedback=False)
    catalog = dict(commands=dict(scope=scope, definitions=[]),
        policy=dict(retrievalEnabled=True, sensitiveDocument=False, allowInSensitiveDocument=False),
        fragments=[dict(id="saved", revision="18446744073709551615", userSha256="a" * 64,
            projectSha256=None, documentSha256=None, trigger="bayes", label="Private fixture", source=r"\frac{α}{}")])
    query = dict(source="中😀\\bay", cursor=11, revision="18446744073709551615", inputFormat="latex", catalog=catalog)
    try:
        assert call("preferences_capabilities")["data"]["defaultEnabled"] is False
        first, second = new(), new()
        for owner in (first, second):
            assert command(owner, "set-policy", policy=policy)["ok"]
        capture = command(first, "capture")["data"]["captureId"]
        other_capture = command(second, "capture")["data"]["captureId"]
        assert capture != other_capture
        snapshot = call("complete_fragments", body=query)["data"]["snapshot"]
        request = dict(query, snapshot=snapshot, candidateId="fragment:saved")
        assert command(second, "record-acceptance", captureId=capture, eventId="1", request=request)["error"]["code"] == "STALE_FEEDBACK"
        result = command(first, "record-acceptance", captureId=capture, eventId="18446744073709551615", request=request)
        assert result["ok"], result
        receipt = result["data"]["receiptId"]
        rank = command(first, "rank-fragments", request=query)
        assert rank["data"]["features"][0]["lastAcceptedEvent"] == "18446744073709551615"
        assert command(first, "undo", receiptId=receipt)["ok"]
        assert command(first, "undo", receiptId=receipt)["error"]["code"] == "STALE_FEEDBACK"
        storage_key = secrets.token_hex(32)
        permission = dict(storageEnabled=True, allowSensitiveStorage=False)
        exported = command(first, "export-checkpoint", storageKeyHex=storage_key,
            checkpoint="18446744073709551615", permission=permission)
        assert exported["ok"], exported
        assert call("preferences_capabilities")["data"]["authenticatedCheckpoints"]
        restored_reply = call("preferences_restore", body=dict(schemaVersion=1, scope=scope,
            keyHex=keys[first], storageKeyHex=storage_key, expectedCheckpoint="18446744073709551615",
            snapshotJson=exported["data"]["snapshotJson"], permission=permission))
        assert restored_reply["ok"], restored_reply
        restored = int(restored_reply["data"]["handle"]); owners.append(restored)
        assert restored not in (first, second)
        assert not any(restored_reply["data"]["info"]["policy"].values())
        assert command(restored, "set-policy", policy=policy)["ok"]
        assert command(restored, "rank-fragments", request=query)["data"]["features"][0]["undone"] == 1
        assert command(restored, "undo", receiptId=receipt)["error"]["code"] == "STALE_FEEDBACK"
        assert command(restored, "export-checkpoint", storageKeyHex=storage_key,
            checkpoint="18446744073709551615", permission=permission)["error"]["code"] == "STALE_CHECKPOINT"
        capture = command(first, "capture")["data"]["captureId"]
        assert command(first, "set-policy", policy=dict(policy, rankingEnabled=False))["ok"]
        assert command(first, "set-policy", policy=policy)["ok"]
        assert command(first, "record-acceptance", captureId=capture, eventId="1", request=request)["error"]["code"] == "STALE_FEEDBACK"
        assert command(first, "clear", generation="1", keyHex=secrets.token_hex(32))["ok"]
        assert command(first, "rank-fragments", request=query)["error"]["code"] == "SCOPE_MISMATCH"
        for raw in (b'\xff', b'{', b'{"kind":"info","PRIVATE_SECRET":true}', b'{"kind":"info","kind":"capture"}'):
            result = call("preferences_command", first, body=raw)
            assert result["error"]["code"] == "INVALID_JSON"
            assert "PRIVATE_SECRET" not in json.dumps(result)
        assert call("preferences_close", first)["ok"]
        assert command(first, "info")["error"]["code"] == "PREFERENCE_NOT_FOUND"
        assert new() != first
    finally:
        for owner in owners:
            call("preferences_close", owner)
    print("Actual preference C ABI: precise IDs, authenticated checkpoints, owner isolation, reset, undo and string ownership passed")


if __name__ == "__main__":
    main()
