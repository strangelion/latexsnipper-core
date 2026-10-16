use latexsnipper_ffi::{application_c::latexsnipper_string_free, formula_preferences_c::*};
use serde_json::Value;
use std::ffi::{c_char, CStr};

fn take(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let parsed = unsafe { serde_json::from_slice(CStr::from_ptr(pointer).to_bytes()) };
    unsafe { latexsnipper_string_free(pointer) };
    parsed.unwrap()
}

#[cfg(not(all(windows, feature = "protected-preferences")))]
#[test]
fn unsupported_factory_is_exported_and_does_not_dereference_input() {
    let reply = take(unsafe {
        latexsnipper_formula_preferences_open_windows(std::ptr::null(), usize::MAX)
    });
    assert_eq!(reply["error"]["code"], "PREFERENCE_STORAGE_UNAVAILABLE");
    let reply = take(latexsnipper_formula_preferences_capabilities());
    assert_eq!(reply["data"]["protectedStorage"]["available"], false);
    assert_eq!(reply["data"]["persistence"], false);
}

#[cfg(all(windows, feature = "protected-preferences"))]
mod native {
    use super::*;
    use latexsnipper_conversion::formula_completion::preferences::persistence::transactions::FormulaPreferenceStorageIdentity;
    use latexsnipper_ffi::formula_completion_c::latexsnipper_formula_complete_fragments;
    use latexsnipper_preference_storage::windows::{
        WindowsPreferenceCheckpoint, WindowsPreferencePersistence,
    };
    use serde_json::json;
    use std::{cell::RefCell, path::PathBuf, sync::Mutex, time::Duration};
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NOT_FOUND},
        Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC},
    };

    static SERIAL: Mutex<()> = Mutex::new(());
    fn permission(sensitive: bool) -> Value {
        json!({"storageEnabled":true,"allowSensitiveStorage":sensitive})
    }
    fn disabled() -> Value {
        json!({"storageEnabled":false,"allowSensitiveStorage":false})
    }
    fn identity() -> FormulaPreferenceStorageIdentity {
        FormulaPreferenceStorageIdentity {
            user_sha256: "a".repeat(64),
            project_sha256: None,
            document_sha256: Some("c".repeat(64)),
        }
    }
    fn open_raw(bytes: &[u8]) -> Value {
        take(unsafe { latexsnipper_formula_preferences_open_windows(bytes.as_ptr(), bytes.len()) })
    }
    fn open(value: Value) -> Value {
        open_raw(value.to_string().as_bytes())
    }
    struct Fixture {
        directory: Option<tempfile::TempDir>,
        backend: Option<WindowsPreferenceCheckpoint>,
        handles: RefCell<Vec<u64>>,
        namespaces: RefCell<Vec<String>>,
    }
    impl Fixture {
        fn new() -> Self {
            let mut builder = tempfile::Builder::new();
            builder.prefix("snipper-c-preference-fixture-");
            let directory = match std::env::var_os("SNIPPER_PREFERENCE_FIXTURE_PARENT") {
                Some(parent) => builder.tempdir_in(parent).unwrap(),
                None => builder.tempdir().unwrap(),
            };
            let backend = WindowsPreferenceCheckpoint::create_private_root(
                directory.path(),
                "private",
                "c-storage-fixture",
                identity(),
                WindowsPreferencePersistence::LogonSession,
                Duration::from_secs(10),
            )
            .unwrap();
            Self {
                directory: Some(directory),
                backend: Some(backend),
                handles: RefCell::new(Vec::new()),
                namespaces: RefCell::new(Vec::new()),
            }
        }
        fn root(&self) -> PathBuf {
            self.directory.as_ref().unwrap().path().join("private")
        }
        fn request(&self, initialize: bool) -> Value {
            let action = if initialize {
                json!({"kind":"initialize-existing","rootPath":self.root(),"generation":"0"})
            } else {
                json!({"kind":"open-existing","rootPath":self.root()})
            };
            json!({"schemaVersion":1,"owner":"c-storage-fixture",
                "identity":{"userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":"c".repeat(64)},
                "persistence":"logon-session","operationTimeoutMs":10000,"permission":permission(false),"action":action})
        }
        fn adopt(&self, reply: Value) -> Owner {
            assert_eq!(reply["ok"], true, "{reply}");
            let id = reply["data"]["handle"].as_str().unwrap().parse().unwrap();
            let namespace = reply["data"]["info"]["protectedStorage"]["ownerSha256"]
                .as_str()
                .unwrap();
            assert!(namespace.len() == 64 && namespace.bytes().all(|b| b.is_ascii_hexdigit()));
            if !self.namespaces.borrow().iter().any(|n| n == namespace) {
                self.namespaces.borrow_mut().push(namespace.into());
            }
            self.handles.borrow_mut().push(id);
            Owner(id)
        }
        fn owner(&self, initialize: bool) -> Owner {
            self.adopt(open(self.request(initialize)))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            for handle in self.handles.get_mut().drain(..) {
                take(latexsnipper_formula_preferences_close(handle));
            }
            if let Some(backend) = self.backend.take() {
                let mut namespaces = std::mem::take(self.namespaces.get_mut());
                namespaces.push(backend.owner_sha256().into());
                namespaces.sort();
                namespaces.dedup();
                for namespace in namespaces {
                    let target: Vec<_> = format!("LaTeXSnipperCore.Preferences.v1/{namespace}")
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                    // SAFETY: only exact namespaces of this fixture's fresh
                    // physical roots are deleted. Never enumerate credentials.
                    let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0
                        || unsafe { GetLastError() } == ERROR_NOT_FOUND;
                    if !std::thread::panicking() {
                        assert!(ok, "Owned fixture credential cleanup failed");
                    }
                }
                drop(backend);
            }
            if let Some(directory) = self.directory.take() {
                let result = directory.close();
                if !std::thread::panicking() {
                    result.unwrap();
                }
            }
        }
    }
    struct Owner(u64);
    impl Owner {
        fn raw(&self, bytes: &[u8]) -> Value {
            take(unsafe {
                latexsnipper_formula_preferences_command(self.0, bytes.as_ptr(), bytes.len())
            })
        }
        fn call(&self, value: Value) -> Value {
            self.raw(value.to_string().as_bytes())
        }
        fn info(&self) -> Value {
            self.call(json!({"kind":"info"}))["data"].clone()
        }
        fn enable(&self, sensitive: bool) {
            assert_eq!(self.call(json!({"kind":"set-policy","policy":{"rankingEnabled":true,"feedbackEnabled":true,"allowSensitiveFeedback":sensitive}}))["ok"],true);
        }
        fn capture(&self) -> Value {
            self.call(json!({"kind":"capture"}))["data"]["captureId"].clone()
        }
        fn prepare(&self) -> Value {
            let result =
                self.call(json!({"kind":"storage-prepare","permission":permission(false)}));
            assert_eq!(result["ok"], true, "{result}");
            result["data"].clone()
        }
        fn commit(&self, stage: &Value) -> Value {
            self.call(json!({"kind":"storage-commit","stageId":stage["stageId"],"permission":permission(false)}))
        }
        fn record(&self, event: &str, sensitive: bool) -> Value {
            let query = query(sensitive);
            let bytes = query.to_string();
            let result = take(unsafe {
                latexsnipper_formula_complete_fragments(bytes.as_ptr(), bytes.len())
            });
            assert_eq!(result["ok"], true, "{result}");
            let mut request = query;
            request["snapshot"] = result["data"]["snapshot"].clone();
            request["candidateId"] = json!("fragment:second");
            let result = self.call(json!({"kind":"record-acceptance","captureId":self.capture(),"eventId":event,"request":request}));
            assert_eq!(result["ok"], true, "{result}");
            result["data"]["receiptId"].clone()
        }
    }
    fn query(sensitive: bool) -> Value {
        json!({"source":r"\loc","cursor":4,"revision":"18446744073709551615","inputFormat":"latex",
            "catalog":{"commands":{"scope":{"userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":"c".repeat(64),"generation":"0"},"definitions":[]},
            "policy":{"retrievalEnabled":true,"sensitiveDocument":sensitive,"allowInSensitiveDocument":sensitive},
            "fragments":[{"id":"first","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localfirst","label":"First","source":"x"},
              {"id":"second","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localsecond","label":"PRIVATE_LABEL","source":r"\frac{α}{}"}]}})
    }

    #[test]
    fn native_save_restore_uses_actual_owner_and_preserves_post_prepare_feedback() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        let owner = fixture.owner(true);
        assert_eq!(owner.info()["persistence"], true);
        assert_eq!(owner.info()["policy"]["rankingEnabled"], false);
        owner.enable(false);
        let receipt = owner.record("9007199254740993", false);
        let stage = owner.prepare();
        assert_eq!(stage["checkpoint"], "1");
        assert_eq!(stage["revision"], "1");
        owner.record("18446744073709551615", false);
        assert_eq!(owner.commit(&stage)["data"]["status"], "applied");
        assert_eq!(owner.info()["eventCount"], 2);
        assert_eq!(
            owner.commit(&stage)["error"]["code"],
            "PREFERENCE_STAGE_NOT_FOUND"
        );
        let restored = fixture.owner(false);
        assert_eq!(restored.info()["eventCount"], 1);
        assert_eq!(restored.info()["policy"]["feedbackEnabled"], false);
        assert_eq!(
            restored.call(json!({"kind":"undo","receiptId":receipt}))["error"]["code"],
            "STALE_FEEDBACK"
        );
        restored.enable(false);
        let result = restored.call(json!({"kind":"rank-fragments","request":query(false)}));
        assert_eq!(result["data"]["order"], json!([1, 0]));
        assert_eq!(
            result["data"]["features"][1]["lastAcceptedEvent"],
            "9007199254740993"
        );
        let reply = owner.info().to_string();
        for forbidden in ["keyHex", "snapshotJson", "feedbackKey", "PRIVATE_LABEL"] {
            assert!(!reply.contains(forbidden));
        }
        assert_eq!(
            take(latexsnipper_formula_preferences_close(owner.0))["ok"],
            true
        );
        assert_eq!(fixture.owner(false).info()["eventCount"], 1); // close never flushes event 2
    }

    #[test]
    fn clear_rotates_epoch_with_storage_off_invalidates_tokens_and_protects_raw_paths() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        let owner = fixture.owner(true);
        owner.enable(false);
        let receipt = owner.record("1", false);
        let capture = owner.capture();
        let first = owner.prepare();
        assert_eq!(owner.commit(&first)["ok"], true);
        for command in [
            json!({"kind":"clear","generation":"1","keyHex":"ab".repeat(32)}),
            json!({"kind":"export-checkpoint","storageKeyHex":"cd".repeat(32),"checkpoint":"2","permission":permission(false)}),
        ] {
            assert_eq!(
                owner.call(command)["error"]["code"],
                "PREFERENCE_STORAGE_MANAGED"
            );
        }
        let reply =
            owner.call(json!({"kind":"storage-prepare-clear","generation":"9007199254740993"}));
        assert_eq!(reply["ok"], true, "{reply}");
        let stage = &reply["data"];
        let reply = owner.call(
            json!({"kind":"storage-commit","stageId":stage["stageId"],"permission":disabled()}),
        );
        assert_eq!(reply["ok"], true, "{reply}");
        let info = owner.info();
        assert_eq!(info["eventCount"], 0);
        assert_eq!(info["captureCount"], 0);
        assert_eq!(info["receiptCount"], 0);
        assert_eq!(info["scope"]["generation"], "9007199254740993");
        assert_eq!(info["policy"]["rankingEnabled"], true);
        assert_eq!(
            owner.call(json!({"kind":"undo","receiptId":receipt}))["error"]["code"],
            "STALE_FEEDBACK"
        );
        assert_eq!(
            owner.call(json!({"kind":"discard-capture","captureId":capture}))["error"]["code"],
            "STALE_FEEDBACK"
        );
        let reopened = fixture.owner(false);
        assert_eq!(reopened.info()["eventCount"], 0);
        assert_eq!(reopened.info()["scope"]["generation"], "9007199254740993");
        assert_eq!(reopened.info()["policy"]["feedbackEnabled"], false);
    }

    #[test]
    fn stage_budget_discard_failed_clear_and_unchanged_reconciliation_are_explicit() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        let owner = fixture.owner(true);
        let stages: Vec<_> = (0..4).map(|_| owner.prepare()).collect();
        assert_eq!(
            owner.call(json!({"kind":"storage-prepare","permission":permission(false)}))["error"]
                ["code"],
            "LIMIT_EXCEEDED"
        );
        assert_eq!(
            owner.call(json!({"kind":"storage-prepare-clear","generation":"0"}))["error"]["code"],
            "PREFERENCE_IDENTITY_MISMATCH"
        );
        assert_eq!(owner.info()["protectedStorage"]["pendingStages"], 4);
        assert_eq!(
            owner.call(json!({"kind":"storage-discard-stage","stageId":stages[0]["stageId"]}))
                ["ok"],
            true
        );
        let fifth = owner.prepare();
        assert_eq!(fifth["checkpoint"], "5");
        let result = owner.call(json!({"kind":"storage-reconcile","stageId":fifth["stageId"],"permission":permission(false)}));
        assert_eq!(result["data"]["status"], "unchanged");
        assert_eq!(owner.info()["protectedStorage"]["pendingStages"], 4);
        let reset = owner.call(json!({"kind":"storage-prepare-clear","generation":"1"}));
        assert_eq!(reset["ok"], true);
        assert_eq!(owner.info()["protectedStorage"]["pendingStages"], 1);
        assert_eq!(
            owner.commit(&fifth)["error"]["code"],
            "PREFERENCE_STAGE_NOT_FOUND"
        );
        assert_eq!(owner.commit(&reset["data"])["ok"], true);
    }

    #[test]
    fn current_sensitive_permission_is_rechecked_on_prepare_commit_reconcile_and_open() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        let owner = fixture.owner(true);
        owner.enable(true);
        owner.record("1", true);
        assert_eq!(
            owner.call(json!({"kind":"storage-prepare","permission":permission(false)}))["error"]
                ["code"],
            "SENSITIVE_STORAGE_DENIED"
        );
        let stage = owner.call(json!({"kind":"storage-prepare","permission":permission(true)}))
            ["data"]
            .clone();
        for kind in ["storage-commit", "storage-reconcile"] {
            assert_eq!(
                owner.call(
                    json!({"kind":kind,"stageId":stage["stageId"],"permission":permission(false)})
                )["error"]["code"],
                "SENSITIVE_STORAGE_DENIED"
            );
            assert_eq!(
                owner.call(json!({"kind":kind,"stageId":stage["stageId"],"permission":disabled()}))
                    ["error"]["code"],
                "PREFERENCE_STORAGE_DISABLED"
            );
        }
        assert_eq!(owner.info()["protectedStorage"]["pendingStages"], 1);
        assert_eq!(owner.call(json!({"kind":"storage-commit","stageId":stage["stageId"],"permission":permission(true)}))["ok"],true);
        assert_eq!(
            open(fixture.request(false))["error"]["code"],
            "SENSITIVE_STORAGE_DENIED"
        );
        let mut request = fixture.request(false);
        request["permission"] = permission(true);
        assert_eq!(
            fixture.adopt(open(request)).info()["policy"]["allowSensitiveFeedback"],
            false
        );
    }

    #[test]
    fn competing_owners_cannot_overwrite_and_discard_does_not_remove_recovery_fence() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        let first = fixture.owner(true);
        let second = fixture.owner(true);
        let a = first.prepare();
        let b = second.prepare();
        assert_ne!(a["stageId"], b["stageId"]);
        assert_eq!(
            second.commit(&a)["error"]["code"],
            "PREFERENCE_STAGE_NOT_FOUND"
        );
        assert_eq!(first.commit(&a)["ok"], true);
        let conflict = second.commit(&b);
        assert_eq!(conflict["error"]["code"], "PREFERENCE_CONFLICT");
        assert_eq!(conflict["error"]["recoverable"], false);
        assert_eq!(second.info()["protectedStorage"]["recoveryRequired"], true);
        assert_eq!(second.call(json!({"kind":"storage-reconcile","stageId":b["stageId"],"permission":permission(false)}))["error"]["code"],"PREFERENCE_CONFLICT");
        assert_eq!(
            second.call(json!({"kind":"storage-discard-stage","stageId":b["stageId"]}))["ok"],
            true
        );
        assert_eq!(second.info()["protectedStorage"]["recoveryRequired"], true);
        assert_eq!(
            second.call(json!({"kind":"storage-prepare","permission":permission(false)}))["error"]
                ["code"],
            "PREFERENCE_RECOVERY_REQUIRED"
        );
        let reset = second.call(json!({"kind":"storage-prepare-clear","generation":"1"}));
        assert_eq!(reset["ok"], true);
        assert_eq!(second.commit(&reset["data"])["ok"], true);
        assert_eq!(second.info()["protectedStorage"]["recoveryRequired"], false);
        assert_eq!(fixture.owner(false).info()["scope"]["generation"], "1");
    }

    #[test]
    fn malformed_closed_requests_redact_private_fields_and_never_provision() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        let base = fixture.request(true).to_string();
        for request in [
            base.replacen("{", r#"{"schemaVersion":1,"#, 1),
            base.replacen("{", r#"{"PRIVATE_UNKNOWN":"SECRET","#, 1),
            base.replace(r#""generation":"0""#, r#""generation":"01""#),
            base.replace(r#""generation":"0""#, r#""generation":0"#),
            base.replace(
                r#""storageEnabled":true"#,
                r#""storageEnabled":true,"storageEnabled":false"#,
            ),
            base.clone() + "trailing",
        ] {
            let reply = open_raw(request.as_bytes());
            assert_eq!(reply["error"]["code"], "INVALID_JSON", "{reply}");
            for private in [
                "SECRET",
                "PRIVATE_UNKNOWN",
                fixture.root().to_str().unwrap(),
            ] {
                assert!(!reply.to_string().contains(private));
            }
        }
        let reply =
            take(unsafe { latexsnipper_formula_preferences_open_windows(std::ptr::null(), 0) });
        assert_eq!(reply["error"]["code"], "INVALID_ARGUMENT");
        assert_eq!(
            open_raw(&vec![b' '; 16 * 1024 + 1])["error"]["code"],
            "INVALID_ARGUMENT"
        );
        let mut request = fixture.request(true);
        request["permission"] = disabled();
        request["action"] = json!({"kind":"create-new","parentPath":fixture.directory.as_ref().unwrap().path(),"directoryName":"MUST_NOT_EXIST","generation":"0"});
        assert_eq!(
            open(request)["error"]["code"],
            "PREFERENCE_STORAGE_DISABLED"
        );
        assert!(!fixture
            .directory
            .as_ref()
            .unwrap()
            .path()
            .join("MUST_NOT_EXIST")
            .exists());
        assert!(fixture
            .backend
            .as_ref()
            .unwrap()
            .read_current_record()
            .unwrap()
            .is_none());
        let owner = fixture.owner(true);
        assert_eq!(owner.raw(br#"{"kind":"storage-prepare","permission":{"storageEnabled":true,"allowSensitiveStorage":false},"permission":{"storageEnabled":false,"allowSensitiveStorage":false}}"#)["error"]["code"],"INVALID_JSON");
        assert_eq!(owner.info()["protectedStorage"]["pendingStages"], 0);
    }

    #[test]
    fn missing_corrupt_or_existing_state_never_falls_back_to_initialization() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        assert_eq!(
            open(fixture.request(false))["error"]["code"],
            "PREFERENCE_STORAGE_NOT_FOUND"
        );
        let owner = fixture.owner(true);
        let stage = owner.prepare();
        assert_eq!(owner.commit(&stage)["ok"], true);
        assert_eq!(
            open(fixture.request(true))["error"]["code"],
            "PREFERENCE_ALREADY_INITIALIZED"
        );
        let object = std::fs::read_dir(fixture.root())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "dpapi"))
            .unwrap();
        let original = std::fs::read(&object).unwrap();
        let mut corrupt = original.clone();
        corrupt[0] ^= 1;
        std::fs::write(&object, &corrupt).unwrap();
        assert_eq!(
            open(fixture.request(false))["error"]["code"],
            "PREFERENCE_PROVIDER_UNAVAILABLE"
        );
        assert_eq!(
            open(fixture.request(true))["error"]["code"],
            "PREFERENCE_INTEGRITY_MISMATCH"
        );
        assert_eq!(std::fs::read(&object).unwrap(), corrupt);
        std::fs::write(&object, &original).unwrap();
        assert_eq!(fixture.owner(false).info()["eventCount"], 0);
    }

    #[test]
    fn cleanup_is_bounded_explicit_and_keeps_current_head() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        let owner = fixture.owner(true);
        for _ in 0..3 {
            let stage = owner.prepare();
            assert_eq!(owner.commit(&stage)["ok"], true);
        }
        for maximum in [0, 129] {
            assert_eq!(
                owner.call(json!({"kind":"storage-cleanup","maximum":maximum}))["error"]["code"],
                "INVALID_ARGUMENT"
            );
        }
        let cleaned = owner.call(json!({"kind":"storage-cleanup","maximum":1}));
        assert_eq!(cleaned["ok"], true, "{cleaned}");
        assert_eq!(cleaned["data"]["removed"], 1);
        assert_eq!(
            owner.call(json!({"kind":"storage-cleanup","maximum":128}))["data"]["removed"],
            1
        );
        assert_eq!(fixture.owner(false).info()["eventCount"], 0);
    }

    #[test]
    fn owner_quota_is_reserved_before_native_io_and_failed_factories_release_it() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        let missing = fixture.request(false);
        for _ in 0..70 {
            assert_eq!(
                open(missing.clone())["error"]["code"],
                "PREFERENCE_STORAGE_NOT_FOUND"
            );
        }
        let owners: Vec<_> = (0..64).map(|_| fixture.owner(true)).collect();
        let mut request = fixture.request(true);
        request["action"] = json!({"kind":"create-new","parentPath":fixture.directory.as_ref().unwrap().path(),"directoryName":"MUST_NOT_EXIST","generation":"0"});
        assert_eq!(open(request)["error"]["code"], "LIMIT_EXCEEDED");
        assert!(!fixture
            .directory
            .as_ref()
            .unwrap()
            .path()
            .join("MUST_NOT_EXIST")
            .exists());
        assert_eq!(
            take(latexsnipper_formula_preferences_close(owners[0].0))["ok"],
            true
        );
        fixture.owner(true);
    }

    #[test]
    fn explicit_create_new_supports_both_persistence_modes_without_implicit_publication() {
        let _serial = SERIAL.lock().unwrap();
        let fixture = Fixture::new();
        for (mode, name) in [
            ("logon-session", "fresh-session"),
            ("this-user-this-computer", "fresh-user-computer"),
        ] {
            let mut request = fixture.request(true);
            request["persistence"] = json!(mode);
            request["action"] = json!({"kind":"create-new","parentPath":fixture.directory.as_ref().unwrap().path(),"directoryName":name,"generation":"18446744073709551615"});
            let owner = fixture.adopt(open(request.clone()));
            request["action"] = json!({"kind":"open-existing","rootPath":fixture.directory.as_ref().unwrap().path().join(name)});
            assert_eq!(
                open(request.clone())["error"]["code"],
                "PREFERENCE_STORAGE_NOT_FOUND"
            );
            let stage = owner.prepare();
            assert_eq!(owner.commit(&stage)["ok"], true);
            let reopened = fixture.adopt(open(request));
            assert_eq!(
                reopened.info()["scope"]["generation"],
                "18446744073709551615"
            );
            assert_eq!(
                owner.call(
                    json!({"kind":"storage-prepare-clear","generation":"18446744073709551615"})
                )["error"]["code"],
                "LIMIT_EXCEEDED"
            );
        }
    }
}
