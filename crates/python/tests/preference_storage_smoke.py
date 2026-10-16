"""Exercise actual PyO3 storage, with exact fresh-fixture credential cleanup."""
from __future__ import annotations

from copy import deepcopy
import ctypes
import json
import os
from pathlib import Path
import pickle
import secrets
import shutil
import subprocess
import sys
import tempfile
import threading
import time

import latexsnipper_core as package
from latexsnipper_core import FormulaPreferences, FormulaPreferenceStorageStage, LaTeXSnipperError, complete_formula_fragments

PERMISSION = dict(storageEnabled=True, allowSensitiveStorage=False)
DISABLED = dict(storageEnabled=False, allowSensitiveStorage=False)
ENABLED = dict(rankingEnabled=True, feedbackEnabled=True, allowSensitiveFeedback=False)
SCOPE = dict(userSha256="a" * 64, projectSha256=None, documentSha256="c" * 64, generation="0")


def expect(code, operation):
    try:
        operation()
    except LaTeXSnipperError as error:
        assert error.code == code, (code, error.code)
        assert not error.retryable and error.detail is None and not error.details
        assert "PRIVATE_SECRET" not in str(error)
    else:
        raise AssertionError("Expected " + code)


def query(sensitive=False):
    catalog = dict(commands=dict(scope=SCOPE, definitions=[]),
        policy=dict(retrievalEnabled=True, sensitiveDocument=sensitive, allowInSensitiveDocument=sensitive),
        fragments=[dict(id="first", revision="0", userSha256="a" * 64, projectSha256=None,
            documentSha256=None, trigger="localfirst", label="First", source="x"),
            dict(id="second", revision="0", userSha256="a" * 64, projectSha256=None,
            documentSha256=None, trigger="localsecond", label="PRIVATE_SECRET", source=r"\frac{α}{}")])
    return dict(source="中😀\\loc", cursor=11, revision=2**64-1, catalog=catalog)


def record(owner, event, sensitive=False):
    request = query(sensitive)
    request.update(snapshot=complete_formula_fragments(**request)["snapshot"], candidate_id="fragment:second")
    return owner.record_acceptance_at(owner.capture_context(), **request, event_id=event)


class Fixture:
    def __enter__(self):
        self.directory = Path(tempfile.mkdtemp(prefix="snipper-python-preferences-",
            dir=os.environ.get("SNIPPER_PREFERENCE_FIXTURE_PARENT"))
        )
        self.owners, self.targets = [], set()
        self.request = dict(schemaVersion=1, owner="python-storage-fixture", identity={k:v for k,v in SCOPE.items() if k!="generation"},
            persistence="logon-session", operationTimeoutMs=10000, permission=PERMISSION,
            action=dict(kind="create-new", parentPath=str(self.directory), directoryName="private", generation="0"))
        return self

    def open(self, request=None):
        owner = FormulaPreferences.open_windows(request=deepcopy(self.request if request is None else request))
        self.owners.append(owner)
        namespace = owner.info()["protectedStorage"]["ownerSha256"]
        assert len(namespace)==64 and all(c in "0123456789abcdef" for c in namespace)
        self.targets.add("LaTeXSnipperCore.Preferences.v1/" + namespace)
        return owner

    def existing(self, initialize=False):
        request = deepcopy(self.request)
        request["action"] = dict(kind="initialize-existing" if initialize else "open-existing", rootPath=str(self.directory/"private"))
        if initialize:
            request["action"]["generation"] = "0"
        return request

    def __exit__(self, *exception):
        # Do not let GC remove a private root if an unsettled native call still
        # owns it. On close/credential cleanup failure preserve the exact owned
        # directory for inspection and fail the test instead of racing the write.
        for owner in self.owners:
            owner.close()
        api = ctypes.WinDLL("advapi32", use_last_error=True)
        delete = api.CredDeleteW
        delete.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32]; delete.restype = ctypes.c_int
        for target in self.targets:
            # Exact namespaces of our fresh physical roots only. Never enumerate.
            assert delete(target, 1, 0) or ctypes.get_last_error()==1168, "Owned fixture credential cleanup failed"
        shutil.rmtree(self.directory)
        assert not self.directory.exists()
        return False


def roundtrip():
    with Fixture() as fixture:
        owner = fixture.open()
        assert owner.info()["persistence"] and not any(owner.info()["policy"].values())
        expect("PREFERENCE_STORAGE_NOT_FOUND", lambda: fixture.open(fixture.existing()))
        owner.set_policy(ENABLED)
        receipt = record(owner, 2**53+1)
        stage = owner.prepare_storage(permission=PERMISSION)
        assert isinstance(stage, FormulaPreferenceStorageStage)
        assert (stage.checkpoint, stage.revision, stage.event_count, stage.clear)==(1,1,1,False)
        for value in (stage, owner):
            try:
                pickle.dumps(value)
            except (TypeError, pickle.PicklingError):
                pass
            else:
                raise AssertionError("Native storage must not be pickleable")
        try:
            FormulaPreferenceStorageStage()
        except TypeError:
            pass
        else:
            raise AssertionError("Stage must not be caller-constructible")
        try:
            stage.checkpoint = 10
        except AttributeError:
            pass
        else:
            raise AssertionError("Stage metadata must be read-only")
        assert owner.reconcile_storage(stage, permission=PERMISSION)=="unchanged"
        expect("PREFERENCE_STORAGE_DISABLED", lambda: owner.commit_storage(stage, permission=DISABLED))
        owner.commit_storage(stage, permission=PERMISSION)
        expect("PREFERENCE_STAGE_NOT_FOUND", lambda: owner.commit_storage(stage, permission=PERMISSION))
        record(owner, 2**64-1)
        pending = owner.prepare_storage(permission=PERMISSION)
        owner.close()  # pending and later feedback are not implicitly saved
        request = fixture.existing()
        child = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--read-json-stdin"],
            input=json.dumps(request), capture_output=True, text=True, encoding="utf-8", timeout=60, check=True)
        assert child.stdout.strip()=="Protected Python cold-process restore passed"
        restored = fixture.open(request)
        assert restored.info()["eventCount"]==1 and not any(restored.info()["policy"].values())
        expect("PREFERENCE_STAGE_NOT_FOUND", lambda: restored.commit_storage(pending, permission=PERMISSION))
        expect("STALE_FEEDBACK", lambda: restored.record_undo(receipt))
        restored.set_policy(ENABLED)
        assert restored.rank_fragments(**query())["features"][1]["lastAcceptedEvent"]==str(2**53+1)
        old_capture, old_receipt = restored.capture_context(), record(restored, 2**53+2)
        expect("PREFERENCE_STORAGE_MANAGED", lambda: restored.export_checkpoint(storage_key=secrets.token_bytes(32), checkpoint=2, permission=PERMISSION))
        expect("PREFERENCE_STORAGE_MANAGED", lambda: restored.clear(generation=1, key=secrets.token_bytes(32)))
        clear = restored.prepare_storage_clear(generation=2**53+1)
        restored.commit_storage(clear, permission=DISABLED)
        assert restored.info()["scope"]["generation"]==str(2**53+1) and restored.info()["eventCount"]==0
        assert restored.info()["policy"]==ENABLED
        expect("STALE_FEEDBACK", lambda: restored.record_undo(old_receipt))
        request = query(); request.update(snapshot=complete_formula_fragments(**request)["snapshot"], candidate_id="fragment:second")
        expect("STALE_FEEDBACK", lambda: restored.record_acceptance_at(old_capture, **request, event_id=2**53+3))
        reopened = fixture.open(fixture.existing())
        assert reopened.info()["eventCount"]==0 and reopened.info()["scope"]["generation"]==str(2**53+1)
        assert not any(reopened.info()["policy"].values())
        assert restored.cleanup_storage(maximum=128)["removed"]==1
        for maximum in (0,129,True,1.0,"1",-1,2**64):
            expect("INVALID_ARGUMENT", lambda: restored.cleanup_storage(maximum=maximum))


def bounded_stages_and_conflict():
    with Fixture() as fixture:
        first = fixture.open(); second = fixture.open(fixture.existing(initialize=True))
        stages = [first.prepare_storage(permission=PERMISSION) for _ in range(4)]
        expect("LIMIT_EXCEEDED", lambda: first.prepare_storage(permission=PERMISSION))
        expect("PREFERENCE_IDENTITY_MISMATCH", lambda: first.prepare_storage_clear(generation=0))
        assert first.info()["protectedStorage"]["pendingStages"]==4
        first.discard_storage_stage(stages[0])
        fifth = first.prepare_storage(permission=PERMISSION)
        assert fifth.checkpoint==5
        other = second.prepare_storage(permission=PERMISSION)
        expect("PREFERENCE_STAGE_NOT_FOUND", lambda: second.commit_storage(fifth, permission=PERMISSION))
        first.commit_storage(fifth, permission=PERMISSION)
        expect("PREFERENCE_STAGE_NOT_FOUND", lambda: first.commit_storage(stages[1], permission=PERMISSION))
        expect("PREFERENCE_CONFLICT", lambda: second.commit_storage(other, permission=PERMISSION))
        expect("PREFERENCE_CONFLICT", lambda: second.reconcile_storage(other, permission=PERMISSION))
        second.discard_storage_stage(other)
        assert second.info()["protectedStorage"]["recoveryRequired"]
        expect("PREFERENCE_RECOVERY_REQUIRED", lambda: second.prepare_storage(permission=PERMISSION))
        clear = second.prepare_storage_clear(generation=1)
        second.commit_storage(clear, permission=DISABLED)
        assert not second.info()["protectedStorage"]["recoveryRequired"]
        assert fixture.open(fixture.existing()).info()["scope"]["generation"]=="1"


def sensitive_and_invalid():
    with Fixture() as fixture:
        invalid = deepcopy(fixture.request); invalid["permission"] = DISABLED
        expect("PREFERENCE_STORAGE_DISABLED", lambda: FormulaPreferences.open_windows(request=invalid))
        assert not (fixture.directory/"private").exists()
        for field, value, code in (("schemaVersion",2,"VERSION_MISMATCH"),("PRIVATE_SECRET",True,"INVALID_ARGUMENT"),
                ("operationTimeoutMs",0,"INVALID_ARGUMENT"),("owner","x"*16384,"INVALID_ARGUMENT")):
            invalid = deepcopy(fixture.request); invalid[field]=value
            expect(code, lambda: FormulaPreferences.open_windows(request=invalid))
            assert not (fixture.directory/"private").exists()
        owner = fixture.open(); owner.set_policy(dict(ENABLED, allowSensitiveFeedback=True)); record(owner,1,True)
        for permission in ({}, dict(PERMISSION, PRIVATE_SECRET=True), dict(PERMISSION, storageEnabled=1)):
            expect("INVALID_ARGUMENT",lambda:owner.prepare_storage(permission=permission))
        assert owner.info()["protectedStorage"]["pendingStages"]==0
        expect("SENSITIVE_STORAGE_DENIED", lambda: owner.prepare_storage(permission=PERMISSION))
        sensitive = dict(PERMISSION, allowSensitiveStorage=True)
        stage = owner.prepare_storage(permission=sensitive)
        for method in (owner.commit_storage, owner.reconcile_storage):
            expect("SENSITIVE_STORAGE_DENIED", lambda: method(stage, permission=PERMISSION))
            expect("PREFERENCE_STORAGE_DISABLED", lambda: method(stage, permission=DISABLED))
        owner.commit_storage(stage, permission=sensitive)
        expect("SENSITIVE_STORAGE_DENIED", lambda: fixture.open(fixture.existing()))
        request=fixture.existing(); request["permission"]=sensitive
        assert not any(fixture.open(request).info()["policy"].values())
        for generation in (True,1.0,"1",-1,2**64):
            expect("INVALID_ARGUMENT", lambda: owner.prepare_storage_clear(generation=generation))


def corrupt_state_does_not_reinitialize():
    with Fixture() as fixture:
        owner=fixture.open(); stage=owner.prepare_storage(permission=PERMISSION)
        owner.commit_storage(stage,permission=PERMISSION)
        expect("PREFERENCE_ALREADY_INITIALIZED",lambda:fixture.open(fixture.existing(initialize=True)))
        objects=list((fixture.directory/"private").glob("*.dpapi")); assert len(objects)==1
        original=objects[0].read_bytes(); corrupt=bytes([original[0]^1])+original[1:]
        try:
            objects[0].write_bytes(corrupt)
            expect("PREFERENCE_PROVIDER_UNAVAILABLE",lambda:fixture.open(fixture.existing()))
            expect("PREFERENCE_INTEGRITY_MISMATCH",lambda:fixture.open(fixture.existing(initialize=True)))
            assert objects[0].read_bytes()==corrupt
        finally:
            objects[0].write_bytes(original)
        assert fixture.open(fixture.existing()).info()["eventCount"]==0


def native_io_releases_gil_and_busy_does_not_close():
    import msvcrt
    with Fixture() as fixture:
        owner=fixture.open(); stage=owner.prepare_storage(permission=PERMISSION)
        namespace=owner.info()["protectedStorage"]["ownerSha256"]
        path=fixture.directory/"private"/("pref-"+namespace+".lock")
        results=[]
        def publish():
            try:
                owner.commit_storage(stage,permission=PERMISSION); results.append("applied")
            except LaTeXSnipperError as error:
                results.append(error.code)
        with path.open("r+b",buffering=0) as locked:
            msvcrt.locking(locked.fileno(),msvcrt.LK_NBLCK,1)
            worker=threading.Thread(target=publish); worker.start()
            busy=False
            try:
                deadline=time.monotonic()+5
                while time.monotonic()<deadline and worker.is_alive():
                    try:
                        owner.info()
                    except LaTeXSnipperError as error:
                        assert error.code=="PREFERENCE_BUSY"; busy=True; break
                    time.sleep(.005)
                assert busy,"No live native operation observed; GIL/busy behavior is unproven"
                expect_busy_close=False
                try:
                    owner.close()
                except LaTeXSnipperError as error:
                    assert error.code=="PREFERENCE_BUSY"; expect_busy_close=True
                assert expect_busy_close
                with FormulaPreferences(scope=SCOPE,key=secrets.token_bytes(32)) as independent:
                    assert independent.rank_fragments(**query())["order"]==[0,1]
            finally:
                locked.seek(0); msvcrt.locking(locked.fileno(),msvcrt.LK_UNLCK,1)
                worker.join(timeout=15)
            assert not worker.is_alive() and results==["applied"],results
        assert not owner.closed and fixture.open(fixture.existing()).info()["eventCount"]==0


def smoke():
    assert "FormulaPreferenceStorageStage" in package.__all__
    caps=FormulaPreferences.storage_capabilities()
    assert caps["maxPreparedStages"]==4 and caps["maxOpenRequestBytes"]==16384
    assert not caps["keysReturned"] and not caps["automaticStorage"] and not caps["globalOwnerRegistry"]
    if not caps["available"]:
        expect("PREFERENCE_STORAGE_UNAVAILABLE",lambda:FormulaPreferences.open_windows(request={"PRIVATE_SECRET":"not-a-path"}))
        print("Python protected storage is explicitly unavailable; no IO or consent enabled")
        return
    assert os.name=="nt"
    roundtrip(); bounded_stages_and_conflict(); sensitive_and_invalid(); corrupt_state_does_not_reinitialize(); native_io_releases_gil_and_busy_does_not_close()
    print("Protected Python storage: cold restore, precise opaque stages, consent, clear, conflict, corruption, GIL/busy and exact cleanup passed")


if __name__=="__main__":
    if sys.argv[1:]==["--read-json-stdin"]:
        encoded=sys.stdin.buffer.read(16385); assert len(encoded)<=16384
        with FormulaPreferences.open_windows(request=json.loads(encoded)) as owner:
            assert owner.info()["eventCount"]==1 and not any(owner.info()["policy"].values())
        print("Protected Python cold-process restore passed")
    else:
        smoke()
