"""Exercise formal preference objects against the extracted native wheel."""
from concurrent.futures import ThreadPoolExecutor
from copy import deepcopy
import pickle
import secrets

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
    print("Python preferences: owned opaque objects, policy/reset guards, precise events and isolated concurrency passed")
