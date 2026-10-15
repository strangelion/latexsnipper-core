use latexsnipper_conversion::formula_completion::{
    fragments::*, preferences::persistence::*, preferences::*,
};
use serde_json::{json, Value};

// Deterministic public test keys, not production secrets.
const FEEDBACK_KEY: [u8; 32] = [19; 32];
const STORAGE_KEY: [u8; 32] = [27; 32];
fn permission() -> FormulaPreferenceStoragePermission {
    FormulaPreferenceStoragePermission {
        storage_enabled: true,
        allow_sensitive_storage: false,
    }
}
fn policy() -> FormulaPreferencePolicy {
    FormulaPreferencePolicy {
        ranking_enabled: true,
        feedback_enabled: true,
        allow_sensitive_feedback: false,
    }
}
fn query() -> FormulaFragmentCompletionRequest {
    serde_json::from_value(json!({"source":"中😀\\local","inputFormat":"latex","cursor":13,"revision":"18446744073709551615",
        "catalog":{"commands":{"scope":{"userSha256":"a".repeat(64),"projectSha256":"b".repeat(64),"documentSha256":"c".repeat(64),"generation":"0"},"definitions":[]},
        "policy":{"retrievalEnabled":true,"sensitiveDocument":false,"allowInSensitiveDocument":false},
        "fragments":[{"id":"a","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localfirst","label":"PRIVATE_LABEL_A","source":"x+1"},
        {"id":"z","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localsecond","label":"PRIVATE_LABEL_Z","source":"\\frac{PRIVATE}{BODY}"}]}})).unwrap()
}
fn apply(query: &FormulaFragmentCompletionRequest) -> FormulaFragmentCompletionApplyRequest {
    FormulaFragmentCompletionApplyRequest {
        source: query.source.clone(),
        input_format: query.input_format,
        cursor: query.cursor,
        revision: query.revision,
        catalog: query.catalog.clone(),
        snapshot: query.execute().unwrap().snapshot,
        candidate_id: "fragment:z".into(),
    }
}
fn owner() -> FormulaPreferenceStore {
    let mut store =
        FormulaPreferenceStore::new(query().catalog.commands.scope, FEEDBACK_KEY).unwrap();
    store.set_policy(policy()).unwrap();
    store
}
fn restore(
    snapshot: &str,
    checkpoint: u64,
) -> Result<FormulaPreferenceStore, FormulaPreferenceStorageError> {
    FormulaPreferenceStore::restore_checkpoint(
        query().catalog.commands.scope,
        FEEDBACK_KEY,
        &STORAGE_KEY,
        checkpoint,
        snapshot,
        permission(),
    )
}
fn record(
    store: &mut FormulaPreferenceStore,
    query: &FormulaFragmentCompletionRequest,
    event: u64,
) -> FormulaPreferenceReceipt {
    store
        .record_acceptance_at(&store.capture_context(), &apply(query), event)
        .unwrap()
        .unwrap()
}

#[test]
fn snapshot_round_trip_retains_features_but_not_consent_or_feedback_tokens() {
    let query = query();
    let mut old = owner();
    let receipt = record(&mut old, &query, 1);
    let capture = old.capture_context();
    let snapshot = old
        .export_checkpoint(&STORAGE_KEY, u64::MAX, permission())
        .unwrap();
    for text in [
        "PRIVATE",
        "BODY",
        "localfirst",
        "localsecond",
        "fragment:z",
        "keyHex",
        "rankingEnabled",
        "feedbackEnabled",
    ] {
        assert!(
            !snapshot.contains(text),
            "private source/consent must not be serialized"
        );
    }
    let mut recovered = restore(&snapshot, u64::MAX).unwrap();
    assert_eq!(recovered.policy(), FormulaPreferencePolicy::default());
    assert_eq!(recovered.event_count(), 1);
    assert_eq!(recovered.rank_fragments(&query).unwrap().order, vec![0, 1]);
    recovered.set_policy(policy()).unwrap();
    assert_eq!(
        recovered.rank_fragments(&query).unwrap(),
        old.rank_fragments(&query).unwrap()
    );
    assert_eq!(
        recovered.record_undo(&receipt),
        Err(FormulaPreferenceError::StaleFeedback)
    );
    assert!(matches!(
        recovered.record_acceptance_at(&capture, &apply(&query), 2),
        Err(FormulaPreferenceError::StaleFeedback)
    ));
    assert!(matches!(
        recovered.record_acceptance_at(&recovered.capture_context(), &apply(&query), 1),
        Err(FormulaPreferenceError::StaleFeedback)
    ));
    assert_eq!(
        recovered.export_checkpoint(&STORAGE_KEY, u64::MAX, permission()),
        Err(FormulaPreferenceStorageError::StaleCheckpoint)
    );
}

#[test]
fn storage_is_separate_explicit_permission_and_checks_keys() {
    let mut store = owner();
    let disabled = FormulaPreferenceStoragePermission::default();
    assert_eq!(
        store.export_checkpoint(&STORAGE_KEY, 1, disabled),
        Err(FormulaPreferenceStorageError::Disabled)
    );
    assert!(matches!(
        FormulaPreferenceStore::restore_checkpoint(
            query().catalog.commands.scope,
            FEEDBACK_KEY,
            &STORAGE_KEY,
            1,
            "private invalid JSON",
            disabled
        ),
        Err(FormulaPreferenceStorageError::Disabled)
    ));
    for key in [[0; 32], FEEDBACK_KEY] {
        assert_eq!(
            store.export_checkpoint(&key, 1, permission()),
            Err(FormulaPreferenceStorageError::InvalidKey)
        );
    }
    let snapshot = store
        .export_checkpoint(&STORAGE_KEY, 1, permission())
        .unwrap();
    assert!(restore(&snapshot, 1).is_ok());
    assert!(matches!(
        FormulaPreferenceStore::restore_checkpoint(
            query().catalog.commands.scope,
            [0; 32],
            &STORAGE_KEY,
            1,
            &snapshot,
            permission()
        ),
        Err(FormulaPreferenceStorageError::InvalidKey)
    ));
}

#[test]
fn tampering_or_wrong_keys_cannot_import_feedback() {
    let mut store = owner();
    record(&mut store, &query(), 1);
    let snapshot = store
        .export_checkpoint(&STORAGE_KEY, 7, permission())
        .unwrap();
    for field in [
        "event",
        "undone",
        "digest",
        "sensitive",
        "checkpoint",
        "scope",
        "key-binding",
    ] {
        let mut changed: Value = serde_json::from_str(&snapshot).unwrap();
        match field {
            "event" => changed["body"]["events"][0]["eventId"] = json!("2"),
            "undone" => changed["body"]["events"][0]["undone"] = json!(true),
            "digest" => changed["body"]["events"][0]["digest"] = json!("0".repeat(64)),
            "sensitive" => changed["body"]["events"][0]["sensitive"] = json!(true),
            "checkpoint" => changed["body"]["checkpoint"] = json!("8"),
            "scope" => changed["body"]["scope"]["documentSha256"] = json!("d".repeat(64)),
            _ => changed["body"]["keyBinding"] = json!("0".repeat(64)),
        }
        assert!(
            matches!(
                restore(&changed.to_string(), 7),
                Err(FormulaPreferenceStorageError::AuthenticationFailed)
            ),
            "{field}"
        );
    }
    for (feedback, storage) in [([20; 32], STORAGE_KEY), (FEEDBACK_KEY, [28; 32])] {
        assert!(matches!(
            FormulaPreferenceStore::restore_checkpoint(
                query().catalog.commands.scope,
                feedback,
                &storage,
                7,
                &snapshot,
                permission()
            ),
            Err(FormulaPreferenceStorageError::AuthenticationFailed)
        ));
    }
}

#[test]
fn expected_scope_and_checkpoint_reject_reset_and_rollback_without_mutating_owner() {
    let mut store = owner();
    record(&mut store, &query(), 1);
    let old = store
        .export_checkpoint(&STORAGE_KEY, 1, permission())
        .unwrap();
    let new = store
        .export_checkpoint(&STORAGE_KEY, 2, permission())
        .unwrap();
    for expected in [0, 1, 3] {
        assert!(matches!(
            restore(&new, expected),
            Err(FormulaPreferenceStorageError::StaleCheckpoint)
        ));
    }
    assert!(matches!(
        restore(&old, 2),
        Err(FormulaPreferenceStorageError::StaleCheckpoint)
    ));
    for field in ["user", "project", "document", "generation"] {
        let mut scope = query().catalog.commands.scope;
        match field {
            "user" => scope.user_sha256 = "d".repeat(64),
            "project" => scope.project_sha256 = None,
            "document" => scope.document_sha256 = None,
            _ => scope.generation = 1,
        }
        assert!(matches!(
            FormulaPreferenceStore::restore_checkpoint(
                scope,
                FEEDBACK_KEY,
                &STORAGE_KEY,
                2,
                &new,
                permission()
            ),
            Err(FormulaPreferenceStorageError::ScopeMismatch)
        ));
    }
    store.clear(1, [21; 32]).unwrap();
    assert_eq!(store.event_count(), 0);
    assert!(matches!(
        FormulaPreferenceStore::restore_checkpoint(
            store.scope().clone(),
            [21; 32],
            &STORAGE_KEY,
            1,
            &old,
            permission()
        ),
        Err(FormulaPreferenceStorageError::ScopeMismatch)
    ));
    let cleared = store
        .export_checkpoint(&STORAGE_KEY, 1, permission())
        .unwrap();
    let cleared = FormulaPreferenceStore::restore_checkpoint(
        store.scope().clone(),
        [21; 32],
        &STORAGE_KEY,
        1,
        &cleared,
        permission(),
    )
    .unwrap();
    assert_eq!(cleared.event_count(), 0);
}

#[test]
fn sensitive_feedback_needs_storage_consent_even_after_learning_is_disabled() {
    let mut query = query();
    query.catalog.policy.sensitive_document = true;
    query.catalog.policy.allow_in_sensitive_document = true;
    let mut store = owner();
    store
        .set_policy(FormulaPreferencePolicy {
            allow_sensitive_feedback: true,
            ..policy()
        })
        .unwrap();
    record(&mut store, &query, 1);
    store
        .set_policy(FormulaPreferencePolicy::default())
        .unwrap();
    assert_eq!(
        store.export_checkpoint(&STORAGE_KEY, 1, permission()),
        Err(FormulaPreferenceStorageError::SensitiveStorageDenied)
    );
    let allowed = FormulaPreferenceStoragePermission {
        allow_sensitive_storage: true,
        ..permission()
    };
    let snapshot = store.export_checkpoint(&STORAGE_KEY, 1, allowed).unwrap();
    assert!(matches!(
        restore(&snapshot, 1),
        Err(FormulaPreferenceStorageError::SensitiveStorageDenied)
    ));
    let restored = FormulaPreferenceStore::restore_checkpoint(
        store.scope().clone(),
        FEEDBACK_KEY,
        &STORAGE_KEY,
        1,
        &snapshot,
        allowed,
    )
    .unwrap();
    assert_eq!(restored.event_count(), 1);
    assert_eq!(restored.policy(), FormulaPreferencePolicy::default());
}

#[test]
fn failed_export_does_not_advance_checkpoint_and_import_preserves_order_and_undo() {
    let mut store = owner();
    let query = query();
    let first = record(&mut store, &query, 1);
    record(&mut store, &query, u64::MAX);
    store.record_undo(&first).unwrap();
    assert_eq!(
        store.export_checkpoint(&STORAGE_KEY, 0, permission()),
        Err(FormulaPreferenceStorageError::StaleCheckpoint)
    );
    let snapshot = store
        .export_checkpoint(&STORAGE_KEY, 1, permission())
        .unwrap();
    assert_eq!(
        store.export_checkpoint(&STORAGE_KEY, 1, permission()),
        Err(FormulaPreferenceStorageError::StaleCheckpoint)
    );
    let mut recovered = restore(&snapshot, 1).unwrap();
    recovered.set_policy(policy()).unwrap();
    assert_eq!(
        store.rank_fragments(&query).unwrap(),
        recovered.rank_fragments(&query).unwrap()
    );
    assert_eq!(
        recovered.rank_fragments(&query).unwrap().features[1].last_accepted_event,
        u64::MAX
    );
    assert!(matches!(
        recovered.record_acceptance_at(&recovered.capture_context(), &apply(&query), u64::MAX),
        Err(FormulaPreferenceError::StaleFeedback)
    ));
}

#[test]
fn closed_json_and_budget_failures_are_private_and_bounded() {
    let snapshot = owner()
        .export_checkpoint(&STORAGE_KEY, 1, permission())
        .unwrap();
    for raw in [
        snapshot.replacen("\"body\":", "\"PRIVATE_FIELD\":true,\"body\":", 1),
        snapshot.replacen(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
            1,
        ),
        snapshot.replacen("\"checkpoint\":\"1\"", "\"checkpoint\":1", 1),
        snapshot.replacen("\"checkpoint\":\"1\"", "\"checkpoint\":\"01\"", 1),
        snapshot.replacen(
            "\"checkpoint\":\"1\"",
            "\"checkpoint\":\"18446744073709551616\"",
            1,
        ),
    ] {
        let error = restore(&raw, 1).err().unwrap();
        assert_eq!(error, FormulaPreferenceStorageError::InvalidSnapshot);
        assert!(!error.to_string().contains("PRIVATE_FIELD"));
    }
    assert!(matches!(
        restore(&" ".repeat(FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES + 1), 1),
        Err(FormulaPreferenceStorageError::LimitExceeded)
    ));
    assert!(restore(&snapshot, 1).is_ok());
}

#[test]
fn maximum_event_capacity_round_trips_without_eviction_or_counter_wrap() {
    let mut store = owner();
    let query = query();
    for event in 1..=FORMULA_PREFERENCE_MAX_EVENTS as u64 {
        record(&mut store, &query, event);
    }
    let snapshot = store
        .export_checkpoint(&STORAGE_KEY, 1, permission())
        .unwrap();
    assert!(snapshot.len() <= FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES);
    let mut restored = restore(&snapshot, 1).unwrap();
    restored.set_policy(policy()).unwrap();
    assert_eq!(restored.event_count(), FORMULA_PREFERENCE_MAX_EVENTS);
    assert!(matches!(
        restored.record_acceptance_at(&restored.capture_context(), &apply(&query), 1025),
        Err(FormulaPreferenceError::LimitExceeded)
    ));
    assert_eq!(
        store.rank_fragments(&query).unwrap(),
        restored.rank_fragments(&query).unwrap()
    );
}

#[test]
fn checkpoint_authentication_matches_independent_python_hmac_reference() {
    let snapshot = owner()
        .export_checkpoint(&STORAGE_KEY, 1, permission())
        .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    // Python stdlib hmac/sha256 over the domain and compact typed JSON body.
    assert_eq!(
        snapshot["authentication"],
        "8f1519936504cbdb639d3a8ffd1702f6dbff4bc3947232c04f2f667a5f8ea60a"
    );
}

#[test]
fn host_file_round_trip_rebuilds_a_new_disabled_owner_and_cleans_owned_files() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "latexsnipper-preference-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    struct OwnedFile(std::path::PathBuf);
    impl Drop for OwnedFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0.join("checkpoint.json"));
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    let guard = OwnedFile(directory.clone());
    let mut old = owner();
    record(&mut old, &query(), 1);
    let before = old.rank_fragments(&query()).unwrap();
    let snapshot = old
        .export_checkpoint(&STORAGE_KEY, 1, permission())
        .unwrap();
    std::fs::write(directory.join("checkpoint.json"), snapshot).unwrap();
    drop(old);
    let stored = std::fs::read_to_string(directory.join("checkpoint.json")).unwrap();
    let mut recovered = restore(&stored, 1).unwrap();
    assert_eq!(recovered.policy(), FormulaPreferencePolicy::default());
    recovered.set_policy(policy()).unwrap();
    assert_eq!(recovered.rank_fragments(&query()).unwrap(), before);
    drop(guard);
    assert!(!directory.exists());
}
