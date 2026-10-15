"""Exercise formal preference objects against the extracted native wheel."""
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
import pickle
import secrets
import json
import subprocess
import sys

import latexsnipper_core as package
from latexsnipper_core import (
    FormulaPreferences, FormulaPreferenceCapture, FormulaPreferenceReceipt,
    LaTeXSnipperError, complete_formula_fragments,
)


def expect_error(code, operation):
    try:
        operation()
    except LaTeXSnipperError as error:
        assert error.code == code, (error.code, code)
        assert error.detail is None and not error.details
        assert "PRIVATE_SECRET" not in str(error)
    else:
        raise AssertionError(f"Expected {code}")


def preference_smoke():
    for name in ("FormulaPreferences", "FormulaPreferenceCapture", "FormulaPreferenceReceipt"):
        assert name in package.__all__
    revision = 2**64 - 1
    scope = dict(userSha256="a" * 64, projectSha256="b" * 64, documentSha256="c" * 64, generation="0")
    catalog = dict(commands=dict(scope=scope, definitions=[]),
        policy=dict(retrievalEnabled=True, sensitiveDocument=False, allowInSensitiveDocument=False),
        fragments=[dict(id="first", revision=str(revision), userSha256="a" * 64,
            projectSha256=None, documentSha256=None, trigger="localfirst", label="PRIVATE_SECRET_FIRST", source="x+1"),
            dict(id="second", revision=str(revision), userSha256="a" * 64,
            projectSha256=None, documentSha256=None, trigger="localsecond", label="PRIVATE_SECRET_SECOND", source=r"\frac{α}{}")])
    query = dict(source="中😀\\loc", cursor=11, revision=revision, catalog=catalog)
    reply = complete_formula_fragments(**query)
    accepted = dict(query, snapshot=reply["snapshot"], candidate_id="fragment:second")
    enabled = dict(rankingEnabled=True, feedbackEnabled=True, allowSensitiveFeedback=False)
    with FormulaPreferences(scope=scope, key=secrets.token_bytes(32)) as preferences:
        assert not preferences.closed
        info = preferences.info()
        assert info["schemaVersion"] == 1 and info["maxEvents"] == 1024
        assert info["policy"] == dict(rankingEnabled=False, feedbackEnabled=False, allowSensitiveFeedback=False)
        assert info["inMemory"] and not info["persistence"] and not info["automaticCapture"]
        assert "key" not in info
        assert preferences.rank_fragments(**query)["order"] == [0, 1]
        assert preferences.record_acceptance_at(preferences.capture_context(), **accepted, event_id=1) is None
        preferences.set_policy(enabled)
        capture = preferences.capture_context()
        ticket = preferences.record_acceptance_at(capture, **accepted, event_id=1)
        assert isinstance(capture, FormulaPreferenceCapture) and isinstance(ticket, FormulaPreferenceReceipt)
        ranked = preferences.rank_fragments(**query)
        assert ranked["completion"] == reply and ranked["order"] == [1, 0]
        assert ranked["features"][1] == dict(accepted=1, undone=0, lastAcceptedEvent="1")
        preferences.record_undo(ticket)
        assert preferences.rank_fragments(**query)["order"] == [0, 1]
        expect_error("STALE_FEEDBACK", lambda: preferences.record_undo(ticket))
        expect_error("STALE_FEEDBACK", lambda: preferences.record_acceptance_at(capture, **accepted, event_id=1))
        expect_error("STALE_FEEDBACK", lambda: preferences.record_acceptance_at(capture, **accepted, event_id=0))
        for obj in (preferences, capture, ticket):
            try:
                pickle.dumps(obj)
            except (TypeError, pickle.PicklingError):
                pass
            else:
                raise AssertionError("Private native state must not be pickleable")
            assert "PRIVATE_SECRET" not in repr(obj)
        for cls in (FormulaPreferenceCapture, FormulaPreferenceReceipt):
            try:
                cls()
            except TypeError:
                pass
            else:
                raise AssertionError("Feedback objects cannot be caller-constructed")
        with FormulaPreferences(scope=scope, key=secrets.token_bytes(32)) as other:
            other.set_policy(enabled)
            expect_error("STALE_FEEDBACK", lambda: other.record_acceptance_at(capture, **accepted, event_id=1))
            expect_error("STALE_FEEDBACK", lambda: other.record_undo(ticket))
        # Policy ABA rejects the original queued capture even after enabling again.
        preferences.set_policy(dict(enabled, feedbackEnabled=False))
        preferences.set_policy(enabled)
        expect_error("STALE_FEEDBACK", lambda: preferences.record_acceptance_at(capture, **accepted, event_id=2))
        current = preferences.capture_context()
        # Failed validation must not consume event IDs or mutate state.
        bad = deepcopy(accepted); bad["revision"] = 0
        expect_error("STALE_CONTEXT", lambda: preferences.record_acceptance_at(current, **bad, event_id=2))
        precise = preferences.record_acceptance_at(current, **accepted, event_id=revision)
        assert preferences.rank_fragments(**query)["features"][1]["lastAcceptedEvent"] == str(revision)
        preferences.record_undo(precise)
        sensitive = deepcopy(query)
        sensitive["catalog"]["policy"].update(sensitiveDocument=True, allowInSensitiveDocument=True)
        sensitive_accept = dict(sensitive, snapshot=complete_formula_fragments(**sensitive)["snapshot"], candidate_id="fragment:second")
        assert preferences.record_acceptance_at(current, **sensitive_accept, event_id=revision) is None
        assert preferences.rank_fragments(**sensitive)["features"][1]["accepted"] == 0
        preferences.set_policy(dict(enabled, allowSensitiveFeedback=True))
        preferences.clear(generation=1, key=secrets.token_bytes(32))
        expect_error("STALE_FEEDBACK", lambda: preferences.record_acceptance_at(current, **accepted, event_id=1))
        fresh = deepcopy(query); fresh["catalog"]["commands"]["scope"]["generation"] = "1"
        assert preferences.rank_fragments(**fresh)["features"][1]["accepted"] == 0
        expect_error("SCOPE_MISMATCH", lambda: preferences.rank_fragments(**query))
        fresh["catalog"]["policy"].update(sensitiveDocument=True, allowInSensitiveDocument=True)
        fresh_accept = dict(fresh, snapshot=complete_formula_fragments(**fresh)["snapshot"], candidate_id="fragment:second")
        preferences.record_acceptance_at(preferences.capture_context(), **fresh_accept, event_id=1)
        assert preferences.rank_fragments(**fresh)["order"] == [1, 0]
        expect_error("INVALID_ARGUMENT", lambda: preferences.clear(generation=True, key=secrets.token_bytes(32)))
        expect_error("INVALID_ARGUMENT", lambda: preferences.set_policy(dict(enabled, unexpected=True)))
        expect_error("INVALID_ARGUMENT", lambda: preferences.set_policy(dict(enabled, feedbackEnabled=1)))
        expect_error("INVALID_ARGUMENT", lambda: preferences.rank_fragments(**dict(fresh, revision=True)))
        expect_error("INVALID_ARGUMENT", lambda: preferences.rank_fragments(**dict(fresh, input_format="typst")))
        for invalid in (True, -1, 1.5, "1", 2**64):
            expect_error("INVALID_ARGUMENT", lambda: preferences.record_acceptance_at(
                preferences.capture_context(), **fresh_accept, event_id=invalid))
        huge = deepcopy(fresh)
        template = huge["catalog"]["fragments"][0]
        huge["catalog"]["fragments"] = [dict(template, id=f"entry{i}", source="x %" + '"' * 8188 + "\n") for i in range(60)]
        huge.update(source="x" * 3900 + r"\loc", cursor=3904)
        expect_error("OUTPUT_TOO_LARGE", lambda: preferences.rank_fragments(**huge))
        assert preferences.rank_fragments(**fresh)["order"] == [1, 0]
    assert preferences.closed
    preferences.close()
    expect_error("PREFERENCE_CLOSED", preferences.info)
    expect_error("PREFERENCE_CLOSED", preferences.capture_context)
    expect_error("PREFERENCE_CLOSED", lambda: preferences.__enter__())
    for key in (b"short", bytes(32)):
        expect_error("INVALID_ARGUMENT", lambda: FormulaPreferences(scope=scope, key=key))
    expect_error("INVALID_ARGUMENT", lambda: FormulaPreferences(scope=dict(scope, generation=0), key=secrets.token_bytes(32)))
    # Independent owners can run concurrently without a process-global registry.
    def independent(index):
        with FormulaPreferences(scope=scope, key=secrets.token_bytes(32)) as local:
            local.set_policy(enabled)
            for number in range(index + 1):
                local.record_acceptance_at(local.capture_context(), **accepted, event_id=number + 1)
            return local.rank_fragments(**query)["features"][1]["accepted"]
    with ThreadPoolExecutor(max_workers=4) as pool:
        assert list(pool.map(independent, range(16))) == list(range(1, 17))
    checkpoint_smoke(scope, query, accepted, enabled)
    print("Python preferences: owned opaque objects, policy/reset guards, precise events and isolated concurrency passed")


def checkpoint_smoke(scope, query, accepted, enabled):
    key, storage_key = secrets.token_bytes(32), secrets.token_bytes(32)
    permission = dict(storageEnabled=True, allowSensitiveStorage=False)
    disabled = dict(permission, storageEnabled=False)
    maximum = 2**64 - 1
    with FormulaPreferences(scope=scope, key=key) as original:
        assert original.info()["authenticatedCheckpoints"]
        assert not original.info()["automaticStorage"]
        original.set_policy(enabled)
        old_capture = original.capture_context()
        old_receipt = original.record_acceptance_at(original.capture_context(), **accepted, event_id=9)
        undone = original.record_acceptance_at(original.capture_context(), **accepted, event_id=10)
        original.record_undo(undone)
        def export(checkpoint=maximum, **overrides):
            fields = dict(storage_key=storage_key, checkpoint=checkpoint, permission=permission)
            fields.update(overrides)
            return original.export_checkpoint(**fields)
        expect_error("PREFERENCE_STORAGE_DISABLED", lambda: export(permission=disabled))
        expect_error("INVALID_ARGUMENT", lambda: export(storage_key=key))
        expect_error("INVALID_ARGUMENT", lambda: export(storage_key=bytes(32)))
        for invalid in (True, -1, 1.5, "1", 2**64):
            expect_error("INVALID_ARGUMENT", lambda: export(checkpoint=invalid))
        for invalid in (dict(storageEnabled=True), dict(permission, storageEnabled=1), dict(permission, PRIVATE_SECRET=True)):
            expect_error("INVALID_ARGUMENT", lambda: export(permission=invalid))
        snapshot = export()
        assert isinstance(snapshot, str) and len(snapshot.encode("utf-8")) <= 256 * 1024
        for private in ("PRIVATE_SECRET", "fragment:second", "captureId", "receiptId", key.hex(), storage_key.hex()):
            assert private not in snapshot
        assert json.loads(snapshot)["body"]["checkpoint"] == str(maximum)
        # Restore in a fresh interpreter using trusted parent-supplied scope and
        # freshness, not values copied out of the untrusted snapshot body.
        child = """import json, sys
from latexsnipper_core import FormulaPreferences
data = json.load(sys.stdin)
with FormulaPreferences.restore_checkpoint(scope=data['scope'], key=bytes.fromhex(data['key']),
    storage_key=bytes.fromhex(data['storageKey']), expected_checkpoint=data['checkpoint'],
    snapshot_json=data['snapshot'], permission=data['permission']) as restored:
    assert restored.info()['eventCount'] == 2
    assert not any(restored.info()['policy'].values())
    restored.set_policy(data['policy'])
    assert restored.rank_fragments(**data['query'])['order'] == [1, 0]
print('Fresh-process checkpoint restore passed')
"""
        result = subprocess.run([sys.executable, "-c", child], input=json.dumps(dict(
            scope=scope, key=key.hex(), storageKey=storage_key.hex(), checkpoint=maximum,
            snapshot=snapshot, permission=permission, policy=enabled, query=query)),
            capture_output=True, text=True, timeout=60, check=True)
        assert result.stdout.strip() == "Fresh-process checkpoint restore passed"
        def restore(**overrides):
            fields = dict(scope=scope, key=key, storage_key=storage_key,
                expected_checkpoint=maximum, snapshot_json=snapshot, permission=permission)
            fields.update(overrides)
            return FormulaPreferences.restore_checkpoint(**fields)
        expect_error("STALE_CHECKPOINT", export)
        expect_error("PREFERENCE_STORAGE_DISABLED", lambda: restore(permission=disabled))
        expect_error("AUTHENTICATION_FAILED", lambda: restore(storage_key=secrets.token_bytes(32)))
        expect_error("AUTHENTICATION_FAILED", lambda: restore(key=secrets.token_bytes(32)))
        expect_error("STALE_CHECKPOINT", lambda: restore(expected_checkpoint=maximum-1))
        expect_error("SCOPE_MISMATCH", lambda: restore(scope=dict(scope, generation="1")))
        for invalid in (True, -1, 1.5, "1", 2**64):
            expect_error("INVALID_ARGUMENT", lambda: restore(expected_checkpoint=invalid))
        expect_error("LIMIT_EXCEEDED", lambda: restore(snapshot_json=" " * (256 * 1024 + 1)))
        expect_error("INVALID_SNAPSHOT", lambda: restore(snapshot_json='{"PRIVATE_SECRET":true}'))
        duplicate = snapshot.replace('"schemaVersion":1', '"schemaVersion":1,"schemaVersion":1', 1)
        expect_error("INVALID_SNAPSHOT", lambda: restore(snapshot_json=duplicate))
        changed = json.loads(snapshot); changed["body"]["events"][0]["undone"] = True
        expect_error("AUTHENTICATION_FAILED", lambda: restore(snapshot_json=json.dumps(changed)))
        with restore() as restored:
            assert restored.info()["eventCount"] == 2
            assert not any(restored.info()["policy"].values())
            assert restored.rank_fragments(**query)["order"] == [0, 1]
            restored.set_policy(enabled)
            assert restored.rank_fragments(**query)["features"][1] == dict(accepted=2, undone=1, lastAcceptedEvent="9")
            expect_error("STALE_FEEDBACK", lambda: restored.record_undo(old_receipt))
            expect_error("STALE_FEEDBACK", lambda: restored.record_acceptance_at(old_capture, **accepted, event_id=11))
            capture = restored.capture_context()
            expect_error("STALE_FEEDBACK", lambda: restored.record_acceptance_at(capture, **accepted, event_id=10))
            restored.record_acceptance_at(capture, **accepted, event_id=11)
            expect_error("STALE_CHECKPOINT", lambda: restored.export_checkpoint(
                storage_key=storage_key, checkpoint=maximum, permission=permission))
        expect_error("PREFERENCE_CLOSED", lambda: restored.export_checkpoint(
            storage_key=storage_key, checkpoint=1, permission=permission))
    # A different, explicitly authorized sensitive-storage decision is required.
    with FormulaPreferences(scope=scope, key=key) as sensitive:
        sensitive.set_policy(dict(enabled, allowSensitiveFeedback=True))
        private_query = deepcopy(query)
        private_query["catalog"]["policy"].update(sensitiveDocument=True, allowInSensitiveDocument=True)
        private_accept = dict(private_query, snapshot=complete_formula_fragments(**private_query)["snapshot"], candidate_id="fragment:second")
        sensitive.record_acceptance_at(sensitive.capture_context(), **private_accept, event_id=1)
        sensitive.set_policy(dict(rankingEnabled=False, feedbackEnabled=False, allowSensitiveFeedback=False))
        expect_error("SENSITIVE_STORAGE_DENIED", lambda: sensitive.export_checkpoint(
            storage_key=storage_key, checkpoint=1, permission=permission))
        snapshot = sensitive.export_checkpoint(storage_key=storage_key, checkpoint=1,
            permission=dict(permission, allowSensitiveStorage=True))
        fields = dict(scope=scope, key=key, storage_key=storage_key, expected_checkpoint=1,
            snapshot_json=snapshot, permission=permission)
        expect_error("SENSITIVE_STORAGE_DENIED", lambda: FormulaPreferences.restore_checkpoint(**fields))
        fields["permission"] = dict(permission, allowSensitiveStorage=True)
        with FormulaPreferences.restore_checkpoint(**fields) as restored:
            assert not any(restored.info()["policy"].values())
            assert restored.info()["eventCount"] == 1
    print("Python checkpoints: explicit consent, authenticated restore, fresh owners, precise sequences and sensitive-storage gates passed")
